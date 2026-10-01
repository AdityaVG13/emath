use super::super::{Engine, ConstructorError, Kont, Expr, CValue, ExprKind, parse_int, parse_exact, fault, Arc, BTreeMap, DomainShape, Closure, BinaryOp, Code, ExactInt, exact_fault};
use super::prelude::{
    apply_unary, arrow_domain_of_expr, binary, join_values, domain_expr_of_shape, domain_shape_of_expr,
    domain_shape_of_type, dummy_expr, index_seq, is_fn_type, is_guarded_recur_body, is_schema_tag,
    project_field, schema_tag,
};
use super::super::scalar::decimal_to_rational;
use super::super::interval;
use super::super::world::{self, CarrierWorld};

impl Engine {
    pub(in crate::constructor_layer) fn charge(&mut self) -> Result<(), ConstructorError> {
        self.work += 1;
        if self.work > self.work_limit {
            return Err(ConstructorError {
                code: "budget_exhausted".into(),
                message: "work budget exhausted".into(),
            });
        }
        Ok(())
    }

    // The kont stack is remaining work for a suspended evaluation to
    // resume. An evaluation under `facts_only_memo` never suspends, so
    // it keeps no stack: building each node clones expression subtrees.
    // Every pop then finds the stack empty, which nothing but resume reads.
    pub(in crate::constructor_layer) fn push_kont(&mut self, kont: impl FnOnce() -> Kont) {
        if self.facts_only_memo {
            return;
        }
        if let Some(frame) = self.frames.last_mut() {
            // Unboxed: the kont stack churns one node per engine step;
            // per-node heap boxes were a malloc/free pair each step.
            frame.kont.push(kont());
        }
    }

    pub(in crate::constructor_layer) fn pop_kont(&mut self) -> Option<Kont> {
        self.frames.last_mut()?.kont.pop()
    }

    pub(in crate::constructor_layer) fn eval(
        &mut self,
        expr: &Expr,
    ) -> Result<CValue, ConstructorError> {
        self.visit += 1;
        self.refresh_frame();
        self.charge()?;
        self.eval_fresh(expr)
    }
    pub(in crate::constructor_layer) fn eval_with_kont(
        &mut self,
        expr: &Expr,
        pending: impl FnOnce() -> Kont,
    ) -> Result<CValue, ConstructorError> {
        let position = self
            .frames
            .len()
            .checked_sub(1)
            .map(|index| (index, self.frames[index].kont.len()));
        let result = self.eval(expr);
        if result.is_err() && !self.facts_only_memo {
            if let Some((index, depth)) = position {
                // Insert below the child's pending work, in the original caller
                // frame even when exhaustion left nested call frames above it.
                self.frames[index].kont.insert(depth, pending());
            }
        }
        result
    }

    pub(in crate::constructor_layer) fn eval_fresh(
        &mut self,
        expr: &Expr,
    ) -> Result<CValue, ConstructorError> {
        match &expr.kind {
            ExprKind::Int(text) => parse_int(text),
            ExprKind::Rational { numer, denom } => {
                let num = parse_exact(numer)?;
                let den = parse_exact(denom)?;
                if den.is_zero() {
                    return Err(fault("division_by_zero", "rational denominator is zero"));
                }
                CValue::Rat { num, den }.canon()
            }
            ExprKind::Float(text) => {
                let cleaned = text.trim_end_matches("f64").trim_end_matches("Float64");
                let current = world::current();
                if matches!(
                    current,
                    CarrierWorld::Exact
                        | CarrierWorld::Float32
                        | CarrierWorld::BFloat16
                        | CarrierWorld::Interval
                ) {
                    if let Some((numer, denom)) = decimal_to_rational(cleaned) {
                        let num = parse_exact(&numer)?;
                        let den = parse_exact(&denom)?;
                        if current == CarrierWorld::Exact {
                            return CValue::Rat { num, den }.canon();
                        }
                        if current == CarrierWorld::Interval {
                            let (lo, hi) = interval::enclose_rational(&num, &den)?;
                            return interval::interval(lo, hi);
                        }
                        let x = current.round_rational(&num, &den);
                        if !x.is_finite() {
                            return Err(fault(
                                "non_finite_scalar",
                                format!("{text} overflows the {} carrier", current.name()),
                            ));
                        }
                        return Ok(CValue::Float64(x));
                    }
                }
                cleaned
                    .parse::<f64>()
                    .map(|x| CValue::Float64(current.round(x)))
                    .map_err(|_| fault("invalid_literal", format!("not Float64: {text}")))
            }
            ExprKind::Bool(v) => Ok(CValue::Bool(*v)),
            ExprKind::Str(text) => Ok(CValue::Str(text.clone())),
            ExprKind::Path { segments, .. } => {
                // Single-segment paths are the interpreter's hottest
                // lookup (every variable reference): skip the join's
                // String allocation. A miss still falls through to the
                // joined-name logic and its fault paths unchanged.
                if segments.len() == 1 {
                    if let Some(value) = self.env.get(segments[0].as_str()).cloned() {
                        return Ok(value);
                    }
                }
                let name = segments.join(".");
                if let Some(value) = self.env.get(&name).cloned() {
                    return Ok(value);
                }
                if segments.len() >= 2 {
                    // Nested record projection: fold field projection
                    // through every remaining segment (`enc.next.y`
                    // projects twice). A failed fold at any depth falls
                    // through to the receipt case / unbound fault, so
                    // two-segment behavior is unchanged.
                    if let Some(mut value) = self.env.get(&segments[0]).cloned() {
                        let mut projected = true;
                        for segment in &segments[1..] {
                            if let Some(field) = project_field(&value, segment) { value = field } else {
                                if matches!(value, CValue::Float64(_) | CValue::Interval { .. })
                                    && matches!(segment.as_str(), "numer" | "denom")
                                {
                                    return Err(fault(
                                        "type",
                                        format!("`{}`: {value} has no exact `{segment}`", segments.join(".")),
                                    ));
                                }
                                projected = false;
                                break;
                            }
                        }
                        if projected {
                            return Ok(value);
                        }
                    }
                }
                if segments.len() == 2 {
                    if let Some(value) = self.env.get(&segments[0]) {
                        if let Some(field) = project_field(value, &segments[1]) {
                            return Ok(field);
                        }
                    }
                    if let Some(CValue::Receipt(receipt)) = self.env.get(&segments[0]) {
                        return match segments[1].as_str() {
                            "execution" => Ok(CValue::Record {
                                type_name: receipt.execution.as_str().into(),
                                fields: Arc::new(BTreeMap::new()),
                            }),
                            "fulfillment" => Ok(CValue::Record {
                                type_name: receipt.fulfillment.as_str().into(),
                                fields: Arc::new(BTreeMap::new()),
                            }),
                            "representation" => Ok(CValue::Record {
                                type_name: receipt.representation.as_str().into(),
                                fields: Arc::new(BTreeMap::new()),
                            }),
                            "payload" => Ok(CValue::Record {
                                type_name: receipt.payload.as_deref().unwrap_or_default().into(),
                                fields: Arc::new(BTreeMap::new()),
                            }),
                            "remaining" => Ok(CValue::Sequence(std::sync::Arc::new(
                                receipt
                                    .remaining
                                    .iter()
                                    .map(|item| CValue::Record {
                                        type_name: item.as_str().into(),
                                        fields: Arc::new(BTreeMap::new()),
                                    })
                                    .collect(),
                            ))),
                            "evidence" => Ok(CValue::Sequence(std::sync::Arc::new(
                                receipt
                                    .evidence
                                    .iter()
                                    .map(|item| CValue::Record {
                                        type_name: item.as_str().into(),
                                        fields: Arc::new(BTreeMap::new()),
                                    })
                                    .collect(),
                            ))),
                            _ => Err(fault(
                                "unbound",
                                format!("receipt has no field `{}`", segments[1]),
                            )),
                        };
                    }
                }
                if name == "_unmatched" {
                    return Err(fault(
                        "nonexhaustive_match",
                        "match did not cover the scrutinee",
                    ));
                }
                if name == "true" {
                    return Ok(CValue::Bool(true));
                }
                if name == "false" {
                    return Ok(CValue::Bool(false));
                }
                if is_schema_tag(&name) {
                    return Ok(schema_tag(&name));
                }
                if let Some(decl) = self.functions.get(&name) {
                    // A named function in VALUE position coerces to a
                    // curried closure over its inputs: `f(a, b)` through
                    // the value is `(function a: function b: f(a, b))`
                    // applied to a then b, so the full call re-binds
                    // every input identically. Each closure demands
                    // what its input declares — the same shape the
                    // named lane's admit_input_type checks on every call.
                    let shapes: Vec<DomainShape> = decl
                        .inputs
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            decl.input_types
                                .get(i)
                                .map_or(DomainShape::Unknown, domain_shape_of_type)
                        })
                        .collect();
                    let param = decl.inputs.first().cloned().unwrap_or_default();
                    let mut body = dummy_expr(ExprKind::Call {
                        function: Box::new(dummy_expr(ExprKind::Path {
                            segments: vec![name.clone()],
                            generics: None,
                        })),
                        args: decl
                            .inputs
                            .iter()
                            .map(|input| {
                                dummy_expr(ExprKind::Path {
                                    segments: vec![input.clone()],
                                    generics: None,
                                })
                            })
                            .collect(),
                    });
                    for (input, shape) in decl.inputs.iter().zip(&shapes).skip(1).rev() {
                        body = dummy_expr(ExprKind::FunctionAbs {
                            param: input.clone(),
                            domain: Box::new(domain_expr_of_shape(shape)),
                            body: Box::new(body),
                        });
                    }
                    let domain = shapes.first().cloned().unwrap_or(DomainShape::Unknown);
                    return Ok(CValue::Closure(Arc::new(Closure {
                        param,
                        domain,
                        body,
                        env: BTreeMap::new(),
                        recursive: None,
                        body_key: Default::default(),
                    })));
                }
                if self.expect_atoms.get() && segments.len() == 1 {
                    // Expect-row fault-name atom: `diagnostic.code == <name>`
                    // resolves the fault name nominally; a wrong name makes
                    // the assertion false rather than faulting the example.
                    return Ok(schema_tag(&name));
                }
                Err(fault("unbound", format!("unbound `{name}`")))
            }
            ExprKind::Unary { op, value } => {
                let v = self.eval_with_kont(value, || Kont::UnaryAfter {
                    op: *op,
                    value: value.clone(),
                })?;
                apply_unary(*op, v)
            }
            ExprKind::Binary { op, left, right } => {
                if world::current().splits_data() && world::is_literal_constant(expr) {
                    return world::exact_constant(expr);
                }
                let l = self.eval_with_kont(left, || Kont::BinLeft {
                    op: *op,
                    left: left.clone(),
                    right: right.clone(),
                })?;
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    return match (op, l) {
                        (BinaryOp::And, CValue::Bool(false)) => Ok(CValue::Bool(false)),
                        (BinaryOp::Or, CValue::Bool(true)) => Ok(CValue::Bool(true)),
                        (BinaryOp::And, CValue::Bool(true)) => self.eval(right),
                        (BinaryOp::Or, CValue::Bool(false)) => self.eval(right),
                        _ => Err(fault("type", "boolean combinator expects Bool")),
                    };
                }
                let r = self.eval_with_kont(right, || Kont::BinRight {
                    op: *op,
                    left: l.clone(),
                    right: right.clone(),
                })?;
                binary(*op, l, r)
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let cond = self.eval_with_kont(condition, || Kont::IfAfterCond {
                    condition: condition.clone(),
                    then_value: then_value.clone(),
                    else_value: else_value.clone(),
                })?;
                match cond {
                    CValue::Bool(true) => self.eval_with_kont(then_value, || Kont::IfThen {
                        then_value: then_value.clone(),
                    }),
                    CValue::Bool(false) => self.eval_with_kont(else_value, || Kont::IfElse {
                        else_value: else_value.clone(),
                    }),
                    _ => Err(fault("type", "if condition must be Bool")),
                }
            }
            ExprKind::List(items) => self.eval_seq_items(items, false),
            ExprKind::Tuple(items) => self.eval_seq_items(items, true),
            ExprKind::Record { type_path, fields } if self.facts_only_memo => {
                self.eval_record_direct(type_path, fields)
            }
            ExprKind::Record { type_path, fields } => {
                self.finish_record_fields(type_path.clone(), BTreeMap::new(), fields.clone(), None)
            }
            ExprKind::SequenceCons {
                head,
                tail,
                spread_head,
            } => {
                let spread_head = *spread_head;
                self.push_kont(|| Kont::ConsLeft {
                    head: head.clone(),
                    tail: tail.clone(),
                    spread_head,
                });
                let h = self.eval(head)?;
                self.pop_kont();
                self.push_kont(|| Kont::ConsAfterHead {
                    head: h.clone(),
                    tail: tail.clone(),
                    spread_head,
                });
                let t = self.eval(tail)?;
                self.pop_kont();
                join_values(h, t, spread_head)
            }
            ExprKind::Index { value, indices } => {
                let seq = self.eval(value)?;
                if !matches!(seq, CValue::Sequence(_) | CValue::Buffer(_)) {
                    return Err(fault("type", "index requires a sequence or buffer"));
                }
                let Some(index_expr) = indices.first() else {
                    return Err(fault("invalid_index", "missing index"));
                };
                self.push_kont(|| Kont::IndexAfterSeq {
                    seq: seq.clone(),
                    index: Box::new(index_expr.clone()),
                });
                let CValue::Int(i) = self.eval(index_expr)? else {
                    self.pop_kont();
                    return Err(fault("type", "index must be Int"));
                };
                self.pop_kont();
                index_seq(seq, i)
            }
            ExprKind::Call { function, args } => self.eval_call(function, args),
            ExprKind::FunctionAbs {
                param,
                domain,
                body,
            } => {
                Ok(CValue::Closure(Arc::new(Closure {
                    param: param.clone(),
                    // Record the declared domain so both check lanes see
                    // the same shape (the admission lane compares it to
                    // the declared input type; the runtime lane checks
                    // arguments against it).
                    domain: domain_shape_of_expr(domain),
                    // Destination-first: clone the body into its final field
                    // instead of via a temporary Box owner. Infallible, and
                    // nothing publishes until the enclosing Arc::new commits.
                    body: (**body).clone(),
                    env: self.env.captured_by(body),
                    recursive: None,
                    body_key: Default::default(),
                })))
            }
            ExprKind::Recur { name, ty, body, .. } => {
                if !is_fn_type(ty) {
                    return Err(fault(
                        "type",
                        "recur binder type must be a function type A -> B or a record of function types",
                    ));
                }
                if !is_guarded_recur_body(body) {
                    return Err(fault(
                        "unguarded_recursive_binding",
                        "recur body must be a function literal or a record of function literals",
                    ));
                }
                // The recur binder's own domain: the domain half of the
                // declared arrow (`recur f in A -> B` demands A values),
                // Unknown for the record-of-functions shape.
                let recur_domain = arrow_domain_of_expr(ty);
                if let ExprKind::FunctionAbs { param, body, .. } = &body.kind {
                    let mut env = self.env.captured_by(body);
                    let finished = Closure {
                        param: param.clone(),
                        domain: recur_domain.clone(),
                        // Destination-first: clone the body into its final field
                        // instead of via a temporary Box owner. Infallible, and
                        // nothing publishes until the enclosing Arc::new commits.
                        body: (**body).clone(),
                        env: BTreeMap::new(),
                        recursive: Some(name.clone()),
                        body_key: Default::default(),
                    };
                    env.insert(name.clone(), CValue::Closure(Arc::new(finished.clone())));
                    return Ok(CValue::Closure(Arc::new(Closure {
                        param: param.clone(),
                        domain: recur_domain,
                        // Destination-first: clone the body into its final field
                        // instead of via a temporary Box owner. Infallible, and
                        // nothing publishes until the enclosing Arc::new commits.
                        body: (**body).clone(),
                        env,
                        recursive: Some(name.clone()),
                        body_key: Default::default(),
                    })));
                }
                Ok(CValue::Closure(Arc::new(Closure {
                    param: String::new(),
                    domain: recur_domain,
                    // Destination-first: clone the body into its final field
                    // instead of via a temporary Box owner. Infallible, and
                    // nothing publishes until the enclosing Arc::new commits.
                    body: (**body).clone(),
                    env: self.env.captured_by(body),
                    recursive: Some(name.clone()),
                    body_key: Default::default(),
                })))
            }
            ExprKind::Quote { body } => {
                let expr = self.mint_binds(body);
                let deps = self.dependency_snapshot(&expr);
                Ok(CValue::Code(Box::new(Code { expr, deps })))
            }
            ExprKind::QuoteBind {
                param,
                domain,
                body,
            } => {
                let expr = self.bind_fresh(param, domain, body);
                let deps = self.dependency_snapshot(&expr);
                Ok(CValue::Code(Box::new(Code { expr, deps })))
            }
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => self.eval_callable_binder(callee, param, domain, body),
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                let s = match start.as_ref().map(|e| self.eval(e)).transpose()? {
                    Some(CValue::Int(n)) => n,
                    None => ExactInt::zero(),
                    Some(other) => {
                        return Err(fault(
                            "type",
                            format!("range start must be Int, found {other}"),
                        ));
                    }
                };
                let e = match end.as_ref().map(|e| self.eval(e)).transpose()? {
                    Some(CValue::Int(n)) => n,
                    None => {
                        return Err(fault(
                            "implementation_unavailable",
                            "unbounded range is not a finite sequence",
                        ));
                    }
                    Some(other) => {
                        return Err(fault(
                            "type",
                            format!("range end must be Int, found {other}"),
                        ));
                    }
                };
                let last = if *inclusive {
                    e
                } else {
                    e.sub(&ExactInt::one()).map_err(exact_fault)?
                };
                if last.cmp(&s) == std::cmp::Ordering::Less {
                    return Ok(CValue::Sequence(std::sync::Arc::new(Vec::new())));
                }
                let mut items = Vec::new();
                let mut n = s;
                loop {
                    if items.len() > 1_000_000 {
                        return Err(fault(
                            "overflow",
                            "range exceeds the constructor sequence bound",
                        ));
                    }
                    items.push(CValue::Int(n.clone()));
                    if n.cmp(&last) != std::cmp::Ordering::Less {
                        break;
                    }
                    n = n.add(&ExactInt::one()).map_err(exact_fault)?;
                }
                Ok(CValue::Sequence(std::sync::Arc::new(items)))
            }
            ExprKind::Cases {
                subject,
                arms,
                else_arm,
            } => {
                if let Some(subject) = subject {
                    self.push_kont(|| Kont::MatchWaiting {
                        subject: subject.clone(),
                        arms: arms.clone(),
                        else_arm: else_arm.clone(),
                    });
                    let scrutinee = self.eval(subject)?;
                    self.pop_kont();
                    return self.choose_match(scrutinee, arms, else_arm);
                }
                self.eval_cases_arms(arms, else_arm)
            }
            other => Err(fault(
                "implementation_unavailable",
                format!("constructor seam does not evaluate {other:?}"),
            )),
        }
    }
}

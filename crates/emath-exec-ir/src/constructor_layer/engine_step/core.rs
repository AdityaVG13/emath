use super::super::*;
use super::prelude::{apply_unary, binary, cons_values, index_seq, is_fn_type, is_guarded_recur_body, is_schema_tag, project_field, schema_tag};

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

    pub(in crate::constructor_layer) fn push_kont(&mut self, kont: Kont) {
        if let Some(frame) = self.frames.last_mut() {
            frame.kont.push(Box::new(kont));
        }
    }

    pub(in crate::constructor_layer) fn pop_kont(&mut self) -> Option<Box<Kont>> {
        self.frames.last_mut()?.kont.pop()
    }

    pub(in crate::constructor_layer) fn eval(&mut self, expr: &Expr) -> Result<CValue, ConstructorError> {
        self.visit += 1;
        self.refresh_frame();
        self.charge()?;
        self.eval_fresh(expr)
    }

    pub(in crate::constructor_layer) fn eval_fresh(&mut self, expr: &Expr) -> Result<CValue, ConstructorError> {
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
                cleaned
                    .parse::<f64>()
                    .map(CValue::Float64)
                    .map_err(|_| fault("invalid_literal", format!("not Float64: {text}")))
            }
            ExprKind::Bool(v) => Ok(CValue::Bool(*v)),
            ExprKind::Path { segments, .. } => {
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
                            match project_field(&value, segment) {
                                Some(field) => value = field,
                                None => {
                                    projected = false;
                                    break;
                                }
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
                                type_name: receipt.execution.clone(),
                                fields: BTreeMap::new(),
                            }),
                            "fulfillment" => Ok(CValue::Record {
                                type_name: receipt.fulfillment.clone(),
                                fields: BTreeMap::new(),
                            }),
                            "representation" => Ok(CValue::Record {
                                type_name: receipt.representation.clone(),
                                fields: BTreeMap::new(),
                            }),
                            "payload" => Ok(CValue::Record {
                                type_name: receipt.payload.clone().unwrap_or_default(),
                                fields: BTreeMap::new(),
                            }),
                            "remaining" => Ok(CValue::Sequence(std::sync::Arc::new(
                                receipt
                                    .remaining
                                    .iter()
                                    .map(|item| CValue::Record {
                                        type_name: item.clone(),
                                        fields: BTreeMap::new(),
                                    })
                                    .collect(),
                            ))),
                            "evidence" => Ok(CValue::Sequence(std::sync::Arc::new(
                                receipt
                                    .evidence
                                    .iter()
                                    .map(|item| CValue::Record {
                                        type_name: item.clone(),
                                        fields: BTreeMap::new(),
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
                    return Err(fault("nonexhaustive_match", "match did not cover the scrutinee"));
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
                if self.functions.contains_key(&name) {
                    return Ok(CValue::Closure(Box::new(Closure {
                        param: String::new(),
                        body: expr.clone(),
                        env: self.env.clone(),
                        recursive: Some(name),
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
                self.push_kont(Kont::UnaryAfter {
                    op: *op,
                    value: value.clone(),
                });
                let v = self.eval(value)?;
                self.pop_kont();
                apply_unary(*op, v)
            }
            ExprKind::Binary { op, left, right } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    self.push_kont(Kont::BinLeft {
                        op: *op,
                        left: left.clone(),
                        right: right.clone(),
                    });
                    let l = self.eval(left)?;
                    self.pop_kont();
                    return match (op, l) {
                        (BinaryOp::And, CValue::Bool(false)) => Ok(CValue::Bool(false)),
                        (BinaryOp::Or, CValue::Bool(true)) => Ok(CValue::Bool(true)),
                        (BinaryOp::And, CValue::Bool(true)) => self.eval(right),
                        (BinaryOp::Or, CValue::Bool(false)) => self.eval(right),
                        _ => Err(fault("type", "boolean combinator expects Bool")),
                    };
                }
                self.push_kont(Kont::BinLeft {
                    op: *op,
                    left: left.clone(),
                    right: right.clone(),
                });
                let l = self.eval(left)?;
                self.pop_kont();
                self.push_kont(Kont::BinRight {
                    op: *op,
                    left: l.clone(),
                    right: right.clone(),
                });
                let r = self.eval(right)?;
                self.pop_kont();
                binary(*op, l, r)
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.push_kont(Kont::IfAfterCond {
                    condition: condition.clone(),
                    then_value: then_value.clone(),
                    else_value: else_value.clone(),
                });
                let cond = self.eval(condition)?;
                self.pop_kont();
                match cond {
                    CValue::Bool(true) => {
                        self.push_kont(Kont::IfThen {
                            then_value: then_value.clone(),
                        });
                        let value = self.eval(then_value)?;
                        self.pop_kont();
                        Ok(value)
                    }
                    CValue::Bool(false) => {
                        self.push_kont(Kont::IfElse {
                            else_value: else_value.clone(),
                        });
                        let value = self.eval(else_value)?;
                        self.pop_kont();
                        Ok(value)
                    }
                    _ => Err(fault("type", "if condition must be Bool")),
                }
            }
            ExprKind::List(items) => self.eval_seq_items(items, false),
            ExprKind::Tuple(items) => self.eval_seq_items(items, true),
            ExprKind::Record { type_path, fields } => {
                self.finish_record_fields(type_path.clone(), BTreeMap::new(), fields.clone(), None)
            }
            ExprKind::SequenceCons { head, tail } => {
                self.push_kont(Kont::ConsLeft {
                    head: head.clone(),
                    tail: tail.clone(),
                });
                let h = self.eval(head)?;
                self.pop_kont();
                self.push_kont(Kont::ConsAfterHead {
                    head: h.clone(),
                    tail: tail.clone(),
                });
                let t = self.eval(tail)?;
                self.pop_kont();
                cons_values(h, t)
            }
            ExprKind::Index { value, indices } => {
                let seq = self.eval(value)?;
                if !matches!(seq, CValue::Sequence(_)) {
                    return Err(fault("type", "index requires a sequence"));
                }
                let Some(index_expr) = indices.first() else {
                    return Err(fault("invalid_index", "missing index"));
                };
                self.push_kont(Kont::IndexAfterSeq {
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
            ExprKind::FunctionAbs { param, body, .. } => Ok(CValue::Closure(Box::new(Closure {
                param: param.clone(),
                body: *body.clone(),
                env: self.env.clone(),
                recursive: None,
            }))),
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
                if let ExprKind::FunctionAbs { param, body, .. } = &body.kind {
                    let mut env = self.env.clone();
                    let finished = Closure {
                        param: param.clone(),
                        body: *body.clone(),
                        env: BTreeMap::new(),
                        recursive: Some(name.clone()),
                    };
                    env.insert(name.clone(), CValue::Closure(Box::new(finished.clone())));
                    return Ok(CValue::Closure(Box::new(Closure {
                        param: param.clone(),
                        body: *body.clone(),
                        env,
                        recursive: Some(name.clone()),
                    })));
                }
                Ok(CValue::Closure(Box::new(Closure {
                    param: String::new(),
                    body: *body.clone(),
                    env: self.env.clone(),
                    recursive: Some(name.clone()),
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
                        return Err(fault("type", format!("range start must be Int, found {other}")))
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
                        return Err(fault("type", format!("range end must be Int, found {other}")))
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
                    self.push_kont(Kont::MatchWaiting {
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

use super::super::{Engine, Expr, EvalTail, ConstructorError, ExprKind, Kont, fault, CValue, machine_int_basename, machine_buffer_basename, expect_int, exact_fault, expect_ints, exact_int_sum, exact_int_prod, Rc, BTreeMap, Arc};
use super::prelude::project_field;

impl Engine {
    pub(in crate::constructor_layer) fn eval_tail(
        &mut self,
        expr: &Expr,
    ) -> Result<EvalTail, ConstructorError> {
        match &expr.kind {
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
                let CValue::Bool(flag) = cond else {
                    return Err(fault("type", "if condition must be Bool"));
                };
                if flag {
                    self.eval_tail(then_value)
                } else {
                    self.eval_tail(else_value)
                }
            }
            ExprKind::Call { function, args } => {
                if let ExprKind::Path { segments, .. } = &function.kind {
                    let name = segments.join(".");
                    if machine_int_basename(&name).is_some()
                        || machine_buffer_basename(&name).is_some()
                    {
                        return Ok(EvalTail::Value(self.eval(expr)?));
                    }
                    let local = segments.len() == 1
                        && matches!(self.env.get(&name), Some(CValue::Closure(_)));
                    if !local && self.functions.contains_key(&name) {
                        let vals = self.eval_tail_args(args, |done, rest| Kont::FnCall {
                            name: name.clone(),
                            done,
                            rest,
                        })?;
                        return Ok(EvalTail::Call { name, args: vals });
                    }
                    // Only a local closure in tail position reuses its frame;
                    // other callees retain the ordinary nested-call discipline.
                    if segments.len() == 1 {
                        if let Some(CValue::Closure(_)) = self.env.get(&name) {
                            let callee = self.env.get(&name).cloned().unwrap();
                            let vals = self.eval_tail_args(args, |done, rest| Kont::CallArgs {
                                callee: callee.clone(),
                                done,
                                rest,
                            })?;
                            // `recv.field` parses as `field(recv)`: a
                            // receiver carrying the field projects, as
                            // on the nested-call path.
                            if let [recv] = vals.as_slice()
                                && let Some(field) = project_field(recv, &name)
                            {
                                return Ok(EvalTail::Value(field));
                            }
                            return Ok(EvalTail::Apply { callee, args: vals });
                        }
                    }
                }
                Ok(EvalTail::Value(self.eval(expr)?))
            }
            _ => Ok(EvalTail::Value(self.eval(expr)?)),
        }
    }

    /// A tail call's arguments, left to right, with the pending call on
    /// the continuation while each one evaluates: a budget suspension
    /// inside an argument must resume into the call, not drop it. The
    /// resumed call finishes on the ordinary nested path (same value).
    fn eval_tail_args(
        &mut self,
        args: &[Expr],
        pending: impl Fn(Vec<CValue>, Vec<Rc<Expr>>) -> Kont,
    ) -> Result<Vec<CValue>, ConstructorError> {
        let mut done = Vec::with_capacity(args.len());
        if self.facts_only_memo {
            for arg in args {
                done.push(self.eval(arg)?);
            }
            return Ok(done);
        }
        for (i, arg) in args.iter().enumerate() {
            self.push_kont(|| {
                pending(done.clone(), args[i + 1..].iter().map(|expr| Rc::new(expr.clone())).collect())
            });
            self.push_kont(|| Kont::EvalExpr { expr: Rc::new(arg.clone()) });
            done.push(self.eval(arg)?);
            self.pop_kont();
            self.pop_kont();
        }
        Ok(done)
    }

    /// Machine buffer-carrier ops:
    /// `buffer(size, fill)` builds an in-place,
    /// bounds-checked indexed carrier; `buffer_set(buf, i, v)` writes
    /// through shared references and evaluates to Unit. Reads are the
    /// ordinary checked-index surface (`index_seq`) and `.length`
    /// (`project_field`). The work budget charges per engine step as
    /// everywhere else, so a sieve-scale write loop suspends and
    /// resumes at op granularity.
    pub(in crate::constructor_layer) fn eval_machine_buffer(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> Result<CValue, ConstructorError> {
        match op {
            "buffer" => {
                if args.len() != 2 {
                    return Err(fault("arity", "`buffer` expects a size and a fill value"));
                }
                let size = expect_int(self.eval(&args[0])?, op)?;
                let fill = self.eval(&args[1])?;
                let Some(len) = size.to_usize() else {
                    return Err(fault("invalid_index", "buffer size must be non-negative"));
                };
                // The carrier's own cell bound: 4M
                // cells admits sieve-scale work (Project Euler P10
                // needs 2M) while bounding a single allocation to
                // roughly a quarter-gigabyte of carrier cells.
                if len > 4_000_000 {
                    return Err(fault("overflow", "buffer exceeds the carrier cell bound"));
                }
                Ok(CValue::Buffer(std::sync::Arc::new(std::sync::Mutex::new(
                    vec![fill; len],
                ))))
            }
            "buffer_set" => {
                if args.len() != 3 {
                    return Err(fault(
                        "arity",
                        "`buffer_set` expects a buffer, an index, and a value",
                    ));
                }
                let CValue::Buffer(cell) = self.eval(&args[0])? else {
                    return Err(fault("type", "`buffer_set` expects a buffer"));
                };
                let index = expect_int(self.eval(&args[1])?, op)?;
                let value = self.eval(&args[2])?;
                let Some(slot) = index.to_usize() else {
                    return Err(fault("invalid_index", "buffer index out of range"));
                };
                let mut items = cell.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(target) = items.get_mut(slot) else {
                    return Err(fault("invalid_index", "buffer index out of range"));
                };
                *target = value;
                Ok(CValue::Unit)
            }
            _ => Err(fault(
                "implementation_unavailable",
                format!("unknown buffer op `{op}`"),
            )),
        }
    }

    pub(in crate::constructor_layer) fn eval_machine_int(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> Result<CValue, ConstructorError> {
        match op {
            "int_sum" | "int_prod" => {
                if args.len() != 1 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects one sequence(Int) argument"),
                    ));
                }
                let items = expect_ints(self.eval(&args[0])?, op)?;
                let value = if op == "int_sum" {
                    exact_int_sum(&items)
                } else {
                    exact_int_prod(&items)
                };
                super::super::world::machine_int(value.map_err(exact_fault)?)
            }
            _ => {
                if args.len() != 2 {
                    return Err(fault("arity", format!("`{op}` expects two Int arguments")));
                }
                let left = expect_int(self.eval(&args[0])?, op)?;
                let right = expect_int(self.eval(&args[1])?, op)?;
                let value = match op {
                    "int_quot" => left.quot(&right),
                    "int_rem" => left.rem_euclid(&right),
                    "int_pow" => left.pow(&right),
                    other => {
                        return Err(fault(
                            "unbound",
                            format!("unknown machine integer operation `{other}`"),
                        ));
                    }
                };
                super::super::world::machine_int(value.map_err(exact_fault)?)
            }
        }
    }

    pub(in crate::constructor_layer) fn eval_seq_items(
        &mut self,
        items: &[Expr],
        as_tuple: bool,
    ) -> Result<CValue, ConstructorError> {
        if self.facts_only_memo {
            // No kont stack to resume from: read the items in place.
            let mut done = Vec::with_capacity(items.len());
            for item in items {
                done.push(self.eval(item)?);
            }
            return Ok(if as_tuple {
                CValue::Tuple(done)
            } else {
                CValue::Sequence(std::sync::Arc::new(done))
            });
        }
        self.finish_seq_items(
            as_tuple,
            Vec::new(),
            items.iter().map(|expr| Rc::new(expr.clone())).collect(),
            None,
        )
    }

    pub(in crate::constructor_layer) fn finish_seq_items(
        &mut self,
        as_tuple: bool,
        mut done: Vec<CValue>,
        rest: Vec<Rc<Expr>>,
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        let mut rest = rest;
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(|| Kont::SeqItems {
                as_tuple,
                done: done.clone(),
                rest: rest.clone(),
            });
            self.push_kont(|| Kont::EvalExpr { expr: next.clone() });
            done.push(self.eval(&next)?);
            self.pop_kont();
            self.pop_kont();
        }
        Ok(if as_tuple {
            CValue::Tuple(done)
        } else {
            CValue::Sequence(std::sync::Arc::new(done))
        })
    }

    pub(in crate::constructor_layer) fn finish_record_fields(
        &mut self,
        type_path: Vec<String>,
        mut done: BTreeMap<String, CValue>,
        mut rest: Vec<(String, Expr)>,
        incoming: Option<(String, CValue)>,
    ) -> Result<CValue, ConstructorError> {
        if let Some((name, value)) = incoming {
            done.insert(name, value);
        }
        while !rest.is_empty() {
            let (name, value) = rest.remove(0);
            self.push_kont(|| Kont::RecordFields {
                type_path: type_path.clone(),
                done: done.clone(),
                current: name.clone(),
                current_expr: Box::new(value.clone()),
                rest: rest.clone(),
            });
            self.push_kont(|| Kont::EvalExpr {
                expr: Rc::new(value.clone()),
            });
            done.insert(name, self.eval(&value)?);
            self.pop_kont();
            self.pop_kont();
        }
        self.finish_record(type_path, done)
    }

    /// Evaluate a record literal's fields in place, in source order: the
    /// `finish_record_fields` path without its kont bookkeeping, for an
    /// evaluation that never suspends.
    pub(in crate::constructor_layer) fn eval_record_direct(
        &mut self,
        type_path: &[String],
        fields: &[(String, Expr)],
    ) -> Result<CValue, ConstructorError> {
        let mut done = BTreeMap::new();
        for (name, value) in fields {
            let value = self.eval(value)?;
            done.insert(name.clone(), value);
        }
        self.finish_record(type_path.to_vec(), done)
    }

    pub(in crate::constructor_layer) fn finish_record(
        &mut self,
        type_path: Vec<String>,
        map: BTreeMap<String, CValue>,
    ) -> Result<CValue, ConstructorError> {
        let type_name = type_path.join(".");
        let checked = self.objects.get(&type_name).filter(|schema| !schema.invariants.is_empty()).cloned();
        if let Some(schema) = checked {
            let saved = self.env.clone();
            for (name, value) in &map {
                self.env.insert(name.clone(), value.clone());
            }
            for (name, pred) in &schema.invariants {
                match self.eval(pred)? {
                    CValue::Bool(true) => {}
                    CValue::Bool(false) => {
                        self.env = saved;
                        return Err(fault(
                            "object_invariant_failed",
                            format!("invariant `{name}` failed"),
                        ));
                    }
                    _ => {
                        self.env = saved;
                        return Err(fault(
                            "object_invariant_failed",
                            format!("invariant `{name}` is not Bool"),
                        ));
                    }
                }
            }
            self.env = saved;
            let _ = schema.kind;
        }
        Ok(CValue::Record {
            type_name: type_name.into(),
            fields: Arc::new(map),
        })
    }
}

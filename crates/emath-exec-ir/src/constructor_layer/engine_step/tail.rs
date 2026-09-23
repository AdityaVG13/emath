use super::super::{Engine, Expr, EvalTail, ConstructorError, ExprKind, Kont, fault, CValue, machine_int_basename, machine_buffer_basename, expect_int, exact_fault, expect_ints, exact_int_sum, exact_int_prod, exact_int_sum_from, exact_int_prod_from, exact_int_hamming, exact_int_weighted_prod, ExactInt, exact_int_poly_eval, Rc, BTreeMap, Arc};

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
                    if self.functions.contains_key(&name) {
                        let mut vals = Vec::new();
                        for arg in args {
                            vals.push(self.eval(arg)?);
                        }
                        return Ok(EvalTail::Call { name, args: vals });
                    }
                    // Only a local closure in tail position reuses its frame;
                    // other callees retain the ordinary nested-call discipline.
                    if segments.len() == 1 {
                        if let Some(CValue::Closure(_)) = self.env.get(&name) {
                            let callee = self.env.get(&name).cloned().unwrap();
                            let mut vals = Vec::new();
                            for arg in args {
                                vals.push(self.eval(arg)?);
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
            "int_fact" | "int_double_fact" | "int_totient" => {
                if args.len() != 1 {
                    return Err(fault("arity", format!("`{op}` expects one Int argument")));
                }
                let n = expect_int(self.eval(&args[0])?, op)?;
                let value = match op {
                    "int_fact" => n.factorial(),
                    "int_double_fact" => n.double_factorial(),
                    _ => n.totient(),
                };
                value.map(CValue::Int).map_err(exact_fault)
            }
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
                value.map(CValue::Int).map_err(exact_fault)
            }
            "int_sum_from" | "int_prod_from" => {
                if args.len() != 2 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects a sequence(Int) and an Int start"),
                    ));
                }
                let items = expect_ints(self.eval(&args[0])?, op)?;
                let start = expect_int(self.eval(&args[1])?, op)?;
                let value = if op == "int_sum_from" {
                    exact_int_sum_from(&items, &start)
                } else {
                    exact_int_prod_from(&items, &start)
                };
                value.map(CValue::Int).map_err(exact_fault)
            }
            "int_hamming" => {
                if args.len() != 2 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects two sequence(Int) arguments"),
                    ));
                }
                let left = expect_ints(self.eval(&args[0])?, op)?;
                let right = expect_ints(self.eval(&args[1])?, op)?;
                exact_int_hamming(&left, &right)
                    .map(CValue::Int)
                    .map_err(exact_fault)
            }
            "int_weighted_prod" => {
                if args.len() != 4 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects sequence(Int), sequence(Int), Int, Int"),
                    ));
                }
                let xs = expect_ints(self.eval(&args[0])?, op)?;
                let ws = expect_ints(self.eval(&args[1])?, op)?;
                let start = expect_int(self.eval(&args[2])?, op)?;
                let acc = expect_int(self.eval(&args[3])?, op)?;
                exact_int_weighted_prod(&xs, &ws, &start, &acc)
                    .map(CValue::Int)
                    .map_err(exact_fault)
            }
            "int_egcd" => {
                if args.len() != 2 {
                    return Err(fault("arity", format!("`{op}` expects two Int arguments")));
                }
                let left = expect_int(self.eval(&args[0])?, op)?;
                let right = expect_int(self.eval(&args[1])?, op)?;
                let (g, s, t) = ExactInt::egcd(&left, &right).map_err(exact_fault)?;
                Ok(CValue::Sequence(std::sync::Arc::new(vec![
                    CValue::Int(g),
                    CValue::Int(s),
                    CValue::Int(t),
                ])))
            }
            "int_powmod" | "int_poly_eval" => {
                if args.len() != 3 {
                    return Err(fault("arity", format!("`{op}` expects three arguments")));
                }
                if op == "int_powmod" {
                    let base = expect_int(self.eval(&args[0])?, op)?;
                    let exp = expect_int(self.eval(&args[1])?, op)?;
                    let modulus = expect_int(self.eval(&args[2])?, op)?;
                    base.pow_mod(&exp, &modulus)
                        .map(CValue::Int)
                        .map_err(exact_fault)
                } else {
                    let coeffs = expect_ints(self.eval(&args[0])?, op)?;
                    let point = expect_int(self.eval(&args[1])?, op)?;
                    let modulus = expect_int(self.eval(&args[2])?, op)?;
                    exact_int_poly_eval(&coeffs, &point, &modulus)
                        .map(CValue::Int)
                        .map_err(exact_fault)
                }
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
                    "int_root" => left.floor_root(&right),
                    "int_gcd" => ExactInt::gcd(&left, &right),
                    "int_binom" => left.binomial(&right),
                    "int_pow" => left.pow(&right),
                    "int_rising" => left.rising(&right),
                    "int_falling" => left.falling(&right),
                    other => {
                        return Err(fault(
                            "unbound",
                            format!("unknown machine integer operation `{other}`"),
                        ));
                    }
                };
                value.map(CValue::Int).map_err(exact_fault)
            }
        }
    }

    pub(in crate::constructor_layer) fn eval_seq_items(
        &mut self,
        items: &[Expr],
        as_tuple: bool,
    ) -> Result<CValue, ConstructorError> {
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
            self.push_kont(Kont::SeqItems {
                as_tuple,
                done: done.clone(),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr { expr: next.clone() });
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
            self.push_kont(Kont::RecordFields {
                type_path: type_path.clone(),
                done: done.clone(),
                current: name.clone(),
                current_expr: Box::new(value.clone()),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Rc::new(value.clone()),
            });
            done.insert(name, self.eval(&value)?);
            self.pop_kont();
            self.pop_kont();
        }
        self.finish_record(type_path, done)
    }

    pub(in crate::constructor_layer) fn finish_record(
        &mut self,
        type_path: Vec<String>,
        map: BTreeMap<String, CValue>,
    ) -> Result<CValue, ConstructorError> {
        let type_name = type_path.join(".");
        if let Some(schema) = self.objects.get(&type_name).cloned() {
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
            type_name,
            fields: Arc::new(map),
        })
    }
}

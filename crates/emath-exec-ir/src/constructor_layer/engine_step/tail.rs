use super::super::*;

impl Engine {
    pub(in crate::constructor_layer) fn eval_tail(&mut self, expr: &Expr) -> Result<EvalTail, ConstructorError> {
        match &expr.kind {
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
                    if machine_int_basename(&name).is_some() {
                        return Ok(EvalTail::Value(self.eval(expr)?));
                    }
                    if self.functions.contains_key(&name) {
                        let mut vals = Vec::new();
                        for arg in args {
                            vals.push(self.eval(arg)?);
                        }
                        return Ok(EvalTail::Call { name, args: vals });
                    }
                }
                Ok(EvalTail::Value(self.eval(expr)?))
            }
            _ => Ok(EvalTail::Value(self.eval(expr)?)),
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
                    "int_modinv" => left.mod_inv(&right),
                    "int_sqrt_mod" => left.sqrt_mod(&right),
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
        self.finish_seq_items(as_tuple, Vec::new(), items.to_vec(), None)
    }

    pub(in crate::constructor_layer) fn finish_seq_items(
        &mut self,
        as_tuple: bool,
        mut done: Vec<CValue>,
        mut rest: Vec<Expr>,
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::SeqItems {
                as_tuple,
                done: done.clone(),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Box::new(next.clone()),
            });
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
                expr: Box::new(value.clone()),
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
            fields: map,
        })
    }

}

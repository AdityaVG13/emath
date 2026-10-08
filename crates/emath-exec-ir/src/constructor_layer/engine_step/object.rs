use super::super::{Engine, Expr, CValue, ConstructorError, fault, Arc, BTreeMap, ExactInt, Kont, Rc};
use super::prelude::{eq_values};

impl Engine {
    pub(in crate::constructor_layer) fn pack_object(&mut self, type_name: &str, args: &[Expr]) -> Result<CValue, ConstructorError> {
        self.finish_pack_args(type_name, Vec::with_capacity(args.len()), args, None)
    }

    pub(in crate::constructor_layer) fn finish_pack_args<E: super::tail::PendingExpr>(
        &mut self,
        type_name: &str,
        mut done: Vec<CValue>,
        rest: &[E],
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        self.eval_pending_items(&mut done, rest, |done, rest| Kont::PackArgs {
            type_name: type_name.to_string(), done, rest,
        })?;
        self.finish_pack(type_name, done)
    }

    fn finish_pack(&mut self, type_name: &str, mut vals: Vec<CValue>) -> Result<CValue, ConstructorError> {
        let schema = self
            .objects
            .get(type_name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown object `{type_name}`")))?;
        match schema.kind.as_str() {
            "abstract" => {
                if vals.len() != 1 {
                    return Err(fault("arity", "abstract pack expects one representation"));
                }
                Ok(CValue::Record {
                    type_name: type_name.into(),
                    fields: Arc::new(BTreeMap::from([("repr".into(), vals.remove(0))])),
                })
            }
            "package" => {
                if vals.len() < 2 {
                    return Err(fault("arity", "package pack expects n and data"));
                }
                let n = vals[0].clone();
                let data = vals[1].clone();
                if let (CValue::Int(want), CValue::Sequence(items)) = (&n, &data) {
                    if want != &ExactInt::from(items.len()) {
                        return Err(fault(
                            "invalid_index",
                            format!("T[n] pack length {} != {want}", items.len()),
                        ));
                    }
                }
                self.finish_record(
                    vec![type_name.to_string()],
                    BTreeMap::from([("n".into(), n), ("data".into(), data)]),
                )
            }
            other => Err(fault(
                "type",
                format!("`{type_name}.pack` requires abstract or package representation, found {other}"),
            )),
        }
    }

    pub(in crate::constructor_layer) fn open_object(
        &mut self,
        type_name: &str,
        param: &str,
        packed_expr: &Expr,
        body: &Expr,
    ) -> Result<CValue, ConstructorError> {
        let packed = self.eval_with_kont(packed_expr, || Kont::OpenAfterPacked {
            type_name: type_name.to_string(),
            param: param.to_string(),
            packed: Box::new(packed_expr.clone()),
            body: Box::new(body.clone()),
        })?;
        self.finish_open(type_name, param, body, packed)
    }

    pub(in crate::constructor_layer) fn finish_open(
        &mut self,
        type_name: &str,
        param: &str,
        body: &Expr,
        packed: CValue,
    ) -> Result<CValue, ConstructorError> {
        let CValue::Record {
            type_name: got,
            fields,
        } = &packed
        else {
            return Err(fault("type", "open expects a packed object"));
        };
        if &**got != type_name {
            return Err(fault(
                "type",
                format!("opened `{got}` as `{type_name}`"),
            ));
        }
        let schema_kind = self
            .objects
            .get(type_name)
            .map(|schema| schema.kind.as_str().to_string())
            .unwrap_or_default();
        let opened = if schema_kind == "abstract" {
            fields
                .get("repr")
                .cloned()
                .ok_or_else(|| fault("type", "abstract pack missing representation"))?
        } else {
            packed.clone()
        };
        let saved = self.env.clone();
        self.env.insert(param.to_string(), opened);
        self.eval_scoped(body, saved)
    }

    pub(in crate::constructor_layer) fn choose_match(
        &mut self,
        scrutinee: CValue,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
    ) -> Result<CValue, ConstructorError> {
        self.finish_match_arms(scrutinee, arms, else_arm, None)
    }

    pub(in crate::constructor_layer) fn finish_match_arms(
        &mut self,
        scrutinee: CValue,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
        mut incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        for (i, (cond, value)) in arms.iter().enumerate() {
            let taken = if let Some(result) = incoming.take() {
                match result {
                    CValue::Bool(flag) => flag,
                    other => eq_values(&other, &scrutinee)?,
                }
            } else {
                let position = self.kont_position();
                let result = self.pattern_binds(&scrutinee, cond);
                if result.is_err() {
                    self.insert_konts(position, || {
                        [Kont::MatchArm {
                            scrutinee: scrutinee.clone(),
                            arms: arms[i..].to_vec(),
                            else_arm: Box::new(else_arm.clone()),
                        }]
                    });
                }
                result?
            };
            if taken {
                // The arm's pattern bindings live only in the live env;
                // pin them so a suspension inside the arm body resumes
                // with the bindings its re-evaluation reads.
                self.pin_frame_env();
                return self.eval_with_kont(value, || Kont::EvalExpr {
                    expr: Rc::new(value.clone()),
                });
            }
        }
        self.eval_with_kont(else_arm, || Kont::EvalExpr {
            expr: Rc::new(else_arm.clone()),
        })
    }

    pub(in crate::constructor_layer) fn eval_cases_arms(
        &mut self,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
    ) -> Result<CValue, ConstructorError> {
        for (i, (cond, value)) in arms.iter().enumerate() {
            let cond_value = self.eval_with_kont(cond, || Kont::CasesArm {
                cond: Box::new(cond.clone()),
                value: Box::new(value.clone()),
                rest: arms[i + 1..].to_vec(),
                else_arm: Box::new(else_arm.clone()),
            })?;
            if self.cases_cond_taken(&cond_value)? {
                return self.eval_with_kont(value, || Kont::EvalExpr {
                    expr: Rc::new(value.clone()),
                });
            }
        }
        self.eval_with_kont(else_arm, || Kont::EvalExpr {
            expr: Rc::new(else_arm.clone()),
        })
    }

    pub(in crate::constructor_layer) fn cases_cond_taken(&self, value: &CValue) -> Result<bool, ConstructorError> {
        match value {
            CValue::Bool(true) => Ok(true),
            CValue::Bool(false) => Ok(false),
            other => {
                if eq_values(other, &CValue::Bool(true))? {
                    Ok(true)
                } else {
                    Err(fault("type", "match condition must be Bool"))
                }
            }
        }
    }

}

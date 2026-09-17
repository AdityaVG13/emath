use super::super::*;
use super::prelude::{eq_values};

impl Engine {
    pub(in crate::constructor_layer) fn pack_object(&mut self, type_name: &str, args: &[Expr]) -> Result<CValue, ConstructorError> {
        let schema = self
            .objects
            .get(type_name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown object `{type_name}`")))?;
        let mut vals = Vec::new();
        for arg in args {
            vals.push(self.eval(arg)?);
        }
        match schema.kind.as_str() {
            "abstract" => {
                if vals.len() != 1 {
                    return Err(fault("arity", "abstract pack expects one representation"));
                }
                Ok(CValue::Record {
                    type_name: type_name.into(),
                    fields: BTreeMap::from([("repr".into(), vals.remove(0))]),
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
                let saved = self.env.clone();
                self.env.insert("n".into(), n.clone());
                self.env.insert("data".into(), data.clone());
                for (inv_name, pred) in &schema.invariants {
                    match self.eval(pred)? {
                        CValue::Bool(true) => {}
                        CValue::Bool(false) => {
                            self.env = saved;
                            return Err(fault(
                                "object_invariant_failed",
                                format!("invariant `{inv_name}` failed"),
                            ));
                        }
                        _ => {
                            self.env = saved;
                            return Err(fault(
                                "object_invariant_failed",
                                format!("invariant `{inv_name}` is not Bool"),
                            ));
                        }
                    }
                }
                self.env = saved;
                Ok(CValue::Record {
                    type_name: type_name.into(),
                    fields: BTreeMap::from([("n".into(), n), ("data".into(), data)]),
                })
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
        self.push_kont(Kont::OpenAfterPacked {
            type_name: type_name.to_string(),
            param: param.to_string(),
            packed: Box::new(packed_expr.clone()),
            body: Box::new(body.clone()),
        });
        let packed = self.eval(packed_expr)?;
        self.pop_kont();
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
        if got != type_name {
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
        self.push_kont(Kont::EvalExpr {
            expr: Box::new(body.clone()),
        });
        let result = self.eval(body);
        if result.is_ok() {
            self.pop_kont();
        }
        self.env = saved;
        result
    }

    pub(in crate::constructor_layer) fn choose_match(
        &mut self,
        scrutinee: CValue,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
    ) -> Result<CValue, ConstructorError> {
        for (cond, value) in arms {
            if self.pattern_binds(&scrutinee, cond)? {
                self.push_kont(Kont::EvalExpr {
                    expr: Box::new(value.clone()),
                });
                let result = self.eval(value)?;
                self.pop_kont();
                return Ok(result);
            }
        }
        self.push_kont(Kont::EvalExpr {
            expr: Box::new(else_arm.clone()),
        });
        let result = self.eval(else_arm)?;
        self.pop_kont();
        Ok(result)
    }

    pub(in crate::constructor_layer) fn eval_cases_arms(
        &mut self,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
    ) -> Result<CValue, ConstructorError> {
        let mut rest: Vec<(Expr, Expr)> = arms.to_vec();
        while !rest.is_empty() {
            let (cond, value) = rest.remove(0);
            self.push_kont(Kont::CasesArm {
                cond: Box::new(cond.clone()),
                value: Box::new(value.clone()),
                rest: rest.clone(),
                else_arm: Box::new(else_arm.clone()),
            });
            let cond_value = self.eval(&cond)?;
            self.pop_kont();
            if self.cases_cond_taken(&cond_value)? {
                self.push_kont(Kont::EvalExpr {
                    expr: Box::new(value.clone()),
                });
                let result = self.eval(&value)?;
                self.pop_kont();
                return Ok(result);
            }
        }
        self.push_kont(Kont::EvalExpr {
            expr: Box::new(else_arm.clone()),
        });
        let result = self.eval(else_arm)?;
        self.pop_kont();
        Ok(result)
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

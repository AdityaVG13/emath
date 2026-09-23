use super::super::{Engine, Expr, ExprKind, BTreeMap, CValue, Receipt, ConstructorError, fault};
use super::prelude::{dummy_expr, path_expr, representation_of, substitute_path};

impl Engine {
    pub(in crate::constructor_layer) fn fresh_token(&mut self, spelling: &str) -> String {
        self.next_ref += 1;
        format!("#{spelling}.{}", self.next_ref)
    }

    pub(in crate::constructor_layer) fn bind_fresh(&mut self, param: &str, domain: &Expr, body: &Expr) -> Expr {
        let token = self.fresh_token(param);
        let domain = self.mint_binds(domain);
        let body = substitute_path(&self.mint_binds(body), param, &path_expr(&token), &mut self.next_ref);
        dummy_expr(ExprKind::FunctionAbs {
            param: token,
            domain: Box::new(domain),
            body: Box::new(body),
        })
    }

    pub(in crate::constructor_layer) fn mint_binds(&mut self, expr: &Expr) -> Expr {
        let kind = match &expr.kind {
            ExprKind::QuoteBind {
                param,
                domain,
                body,
            } => return self.bind_fresh(param, domain, body),
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } if matches!(
                &callee.kind,
                ExprKind::Path { segments, .. } if segments.join(".") == "quote.bind"
            ) =>
            {
                return self.bind_fresh(param, domain, body);
            }
            ExprKind::FunctionAbs {
                param,
                domain,
                body,
            } => ExprKind::FunctionAbs {
                param: param.clone(),
                domain: Box::new(self.mint_binds(domain)),
                body: Box::new(self.mint_binds(body)),
            },
            ExprKind::Quote { body } => ExprKind::Quote {
                body: Box::new(self.mint_binds(body)),
            },
            ExprKind::Call { function, args } => ExprKind::Call {
                function: Box::new(self.mint_binds(function)),
                args: args.iter().map(|arg| self.mint_binds(arg)).collect(),
            },
            ExprKind::Binary { op, left, right } => ExprKind::Binary {
                op: *op,
                left: Box::new(self.mint_binds(left)),
                right: Box::new(self.mint_binds(right)),
            },
            ExprKind::Unary { op, value } => ExprKind::Unary {
                op: *op,
                value: Box::new(self.mint_binds(value)),
            },
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => ExprKind::If {
                condition: Box::new(self.mint_binds(condition)),
                then_value: Box::new(self.mint_binds(then_value)),
                else_value: Box::new(self.mint_binds(else_value)),
            },
            ExprKind::List(items) => {
                ExprKind::List(items.iter().map(|item| self.mint_binds(item)).collect())
            }
            ExprKind::Tuple(items) => {
                ExprKind::Tuple(items.iter().map(|item| self.mint_binds(item)).collect())
            },
            ExprKind::Cases {
                subject,
                arms,
                else_arm,
            } => ExprKind::Cases {
                subject: subject.as_ref().map(|subject| Box::new(self.mint_binds(subject))),
                arms: arms
                    .iter()
                    .map(|(cond, value)| (self.mint_binds(cond), self.mint_binds(value)))
                    .collect(),
                else_arm: Box::new(self.mint_binds(else_arm)),
            },
            ExprKind::Recur { name, ty, body } => ExprKind::Recur {
                name: name.clone(),
                ty: Box::new(self.mint_binds(ty)),
                body: Box::new(self.mint_binds(body)),
            },
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => ExprKind::CallableBinder {
                callee: Box::new(self.mint_binds(callee)),
                param: param.clone(),
                domain: Box::new(self.mint_binds(domain)),
                body: Box::new(self.mint_binds(body)),
            },
            other => other.clone(),
        };
        Expr {
            kind,
            source: expr.source,
        }
    }

    pub(in crate::constructor_layer) fn run_query(&mut self, name: &str, inputs: &BTreeMap<String, CValue>) -> Result<Receipt, ConstructorError> {
        let decl = self
            .queries
            .get(name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown query `{name}`")))?;
        let saved = self.env.clone();
        for (key, value) in inputs {
            self.env.insert(key.clone(), value.clone());
        }
        for (name, expr) in &decl.defs {
            let value = self.eval(expr)?;
            self.env.insert(name.clone(), value);
        }
        let receipt = if decl.form == "code" {
            Receipt {
                execution: "returned".into(),
                fulfillment: "satisfied".into(),
                representation: "code".into(),
                payload: Some("code".into()),
                evidence: vec!["direct evaluation of requested code form".into()],
                remaining: Vec::new(),
                diagnostic_code: None,
            }
        } else if decl.method.as_deref() == Some("closed_code") {
            // L1 closed-code evaluation: the query inputs ARE the
            // explicit input environment; refusals are named, never
            // silent defaults.
            let target = self
                .env
                .get("target")
                .cloned()
                .or_else(|| self.env.get("question").cloned());
            match target {
                Some(CValue::Code(code)) => {
                    let order = decl.inputs.clone();
                    match self.eval_closed_code(&code, inputs, &order) {
                        Ok(value) => Receipt {
                            execution: "returned".into(),
                            fulfillment: "satisfied".into(),
                            representation: representation_of(&value),
                            payload: Some(value.to_string()),
                            evidence: vec!["closed-code evaluation (L1)".into()],
                            remaining: Vec::new(),
                            diagnostic_code: None,
                        },
                        Err(err) if err.code == "budget_exhausted" => Receipt {
                            execution: "suspended".into(),
                            fulfillment: "partial".into(),
                            representation: "absent".into(),
                            payload: None,
                            evidence: Vec::new(),
                            remaining: vec!["work budget".into()],
                            diagnostic_code: Some(err.code),
                        },
                        Err(err) => Receipt {
                            execution: "returned".into(),
                            fulfillment: "unmet".into(),
                            representation: "absent".into(),
                            payload: None,
                            evidence: Vec::new(),
                            remaining: vec![err.message.clone()],
                            diagnostic_code: Some(if err.code == "type" {
                                "type_mismatch".into()
                            } else {
                                err.code.clone()
                            }),
                        },
                    }
                }
                Some(_) => Receipt {
                    execution: "returned".into(),
                    fulfillment: "unmet".into(),
                    representation: "code".into(),
                    payload: Some("retained".into()),
                    evidence: Vec::new(),
                    remaining: vec!["closed_code expects a code target".into()],
                    diagnostic_code: Some("type_mismatch".into()),
                },
                None => Receipt {
                    execution: "returned".into(),
                    fulfillment: "unmet".into(),
                    representation: "absent".into(),
                    payload: None,
                    evidence: Vec::new(),
                    remaining: vec!["closed_code expects a code target".into()],
                    diagnostic_code: Some("unbound_code".into()),
                },
            }
        } else if decl.method.as_deref().is_none_or(|method| {
            method == "missing"
                || method == "method_unavailable"
                || !self.functions.contains_key(method)
        }) {
            let target = self
                .env
                .get("target")
                .cloned()
                .or_else(|| self.env.get("question").cloned());
            match target {
                Some(CValue::Code(code)) if decl.form != "value" => match self.eval(&code.expr) {
                    Ok(value) => Receipt {
                        execution: "returned".into(),
                        fulfillment: "satisfied".into(),
                        representation: representation_of(&value),
                        payload: Some(value.to_string()),
                        evidence: vec!["direct evaluation".into()],
                        remaining: Vec::new(),
                        diagnostic_code: None,
                    },
                    Err(err) if err.code == "budget_exhausted" => Receipt {
                        execution: "suspended".into(),
                        fulfillment: "partial".into(),
                        representation: "absent".into(),
                        payload: None,
                        evidence: Vec::new(),
                        remaining: vec!["work budget".into()],
                        diagnostic_code: Some(err.code),
                    },
                    Err(err) => Receipt {
                        execution: "faulted".into(),
                        fulfillment: "unmet".into(),
                        representation: "absent".into(),
                        payload: None,
                        evidence: Vec::new(),
                        remaining: vec![err.message],
                        diagnostic_code: Some(err.code),
                    },
                },
                Some(_) => Receipt {
                    execution: "returned".into(),
                    fulfillment: "unmet".into(),
                    representation: "code".into(),
                    payload: Some("retained".into()),
                    evidence: Vec::new(),
                    remaining: vec!["method_unavailable".into()],
                    diagnostic_code: Some("method_unavailable".into()),
                },
                None => Receipt {
                    execution: "returned".into(),
                    fulfillment: "unmet".into(),
                    representation: "absent".into(),
                    payload: None,
                    evidence: Vec::new(),
                    remaining: vec!["method_unavailable".into()],
                    diagnostic_code: Some("method_unavailable".into()),
                },
            }
        } else if decl.accept.as_deref() == Some("unchecked") {
            let payload = self.env.get("payload").cloned().or_else(|| {
                self.env
                    .values()
                    .next()
                    .cloned()
            });
            Receipt {
                execution: "returned".into(),
                fulfillment: if payload.is_some() {
                    "partial"
                } else {
                    "unmet"
                }
                .into(),
                representation: payload
                    .as_ref().map_or_else(|| "absent".into(), representation_of),
                payload: payload.map(|v| v.to_string()),
                evidence: vec!["unchecked candidate".into()],
                remaining: vec!["global bound missing".into()],
                diagnostic_code: None,
            }
        } else {
            let payload = self.env.get("result").cloned();
            Receipt {
                execution: "returned".into(),
                fulfillment: if payload.is_some() {
                    "satisfied"
                } else {
                    "unmet"
                }
                .into(),
                representation: payload
                    .as_ref().map_or_else(|| "absent".into(), representation_of),
                payload: payload.map(|v| v.to_string()),
                evidence: vec!["authored method".into()],
                remaining: Vec::new(),
                diagnostic_code: None,
            }
        };
        self.env = saved;
        Ok(receipt)
    }

}

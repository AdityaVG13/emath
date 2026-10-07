use super::super::{Engine, CValue, Expr, ConstructorError, ExprKind, Code, fault, Kont};
use super::prelude::{eq_values, is_refused_recipe, open_fragment, dummy_expr};

impl Engine {
    pub(in crate::constructor_layer) fn pattern_binds(
        &mut self,
        scrutinee: &CValue,
        pattern: &Expr,
    ) -> Result<bool, ConstructorError> {
        match (&pattern.kind, scrutinee) {
            (ExprKind::List(items), CValue::Sequence(values))
                if items.is_empty() && values.is_empty() =>
            {
                Ok(true)
            }
            (
                ExprKind::SequenceCons {
                    head,
                    tail,
                    spread_head: false,
                },
                CValue::Sequence(values),
            )
                if !values.is_empty() =>
            {
                if let ExprKind::Path { segments, .. } = &head.kind {
                    if let Some(name) = segments.first() {
                        self.env.insert(name.clone(), values[0].clone());
                    }
                }
                if let ExprKind::Path { segments, .. } = &tail.kind {
                    if let Some(name) = segments.first() {
                        self.env.insert(
                            name.clone(),
                            CValue::Sequence(std::sync::Arc::new(values[1..].to_vec())),
                        );
                    }
                }
                Ok(true)
            }
            (ExprKind::Path { segments, .. }, _) if segments.len() == 1 => {
                let name = &segments[0];
                if matches!(
                    name.as_str(),
                    "true" | "false" | "partial" | "satisfied" | "unmet"
                ) {
                    return eq_values(scrutinee, &self.eval(pattern)?);
                }
                self.env.insert(name.clone(), scrutinee.clone());
                Ok(true)
            }
            _ => {
                let cond = self.eval(pattern)?;
                match cond {
                    CValue::Bool(b) => Ok(b),
                    other => Ok(eq_values(&other, scrutinee)?),
                }
            }
        }
    }

    pub(in crate::constructor_layer) fn eval_callable_binder(
        &mut self,
        callee: &Expr,
        param: &str,
        domain: &Expr,
        body: &Expr,
    ) -> Result<CValue, ConstructorError> {
        if let ExprKind::Path { segments, .. } = &callee.kind {
            let name = segments.join(".");
            if name == "quote.bind" {
                let expr = self.bind_fresh(param, domain, body);
                let deps = self.dependency_snapshot(&expr);
                return Ok(CValue::Code(Box::new(Code { expr, deps })));
            }
            if matches!(name.as_str(), "quote.view" | "quote.open") {
                let value = self.eval_with_kont(domain, || Kont::BindAfterDomain {
                    name: name.clone(),
                    param: param.to_string(),
                    domain: Box::new(domain.clone()),
                    body: Box::new(body.clone()),
                })?;
                return self.finish_quote_binder(&name, param, body, value);
            }
            if segments.last().map(String::as_str) == Some("open") {
                return self.open_object(&segments[0], param, domain, body);
            }
        }
        if let ExprKind::Path { segments, .. } = &callee.kind {
            let name = segments.join(".");
            // A name the user bound (env) is the user's; the recipe refusal
            // is for UNBOUND names, so it must not seize user spellings.
            if !self.functions.contains_key(&name)
                && !self.env.contains_key(&name)
                && is_refused_recipe(&name)
            {
                return Err(fault(
                    "method_unavailable",
                    format!(
                        "`{name}` is an ordinary imported function, not a constructor identity"
                    ),
                ));
            }
        }
        let closure = dummy_expr(ExprKind::FunctionAbs {
            param: param.to_string(),
            domain: Box::new(domain.clone()),
            body: Box::new(body.clone()),
        });
        self.eval_call(callee, &[domain.clone(), closure])
    }

    pub(in crate::constructor_layer) fn finish_quote_binder(
        &mut self,
        name: &str,
        param: &str,
        body: &Expr,
        value: CValue,
    ) -> Result<CValue, ConstructorError> {
        let opened = match name {
            "quote.view" => self.quote_view(value)?,
            "quote.open" => {
                self.check_fragment_scope(&value)?;
                open_fragment(value)?
            }
            _ => return Err(fault("incompatible_checkpoint", "unknown scoped quote binder")),
        };
        let saved = self.env.clone();
        self.env.insert(param.to_string(), opened);
        self.eval_scoped(body, saved)
    }
}

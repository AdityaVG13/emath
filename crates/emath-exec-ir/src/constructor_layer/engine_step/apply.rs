use super::super::*;
use super::prelude::{available_body, body_record, decl_stamp, fragment_term, free_path_names, function_body_expr, rebuild_expr, view_of};

impl Engine {
    pub(in crate::constructor_layer) fn apply_value(&mut self, callee: CValue, args: &[CValue]) -> Result<CValue, ConstructorError> {
        match callee {
            CValue::Closure(clos) => {
                if args.is_empty() {
                    return Ok(CValue::Closure(clos));
                }
                let frame_name = clos
                    .recursive
                    .clone()
                    .unwrap_or_else(|| format!("function {}", clos.param));
                if let Some(value) = self.completed_closure(&clos, args) {
                    return if args.len() > 1 {
                        self.apply_value(value, &args[1..])
                    } else {
                        Ok(value)
                    };
                }
                self.charge()?;
                if self.call_depth >= MAX_CALL_DEPTH {
                    return Err(self.recursion_depth_exceeded(&frame_name));
                }
                self.call_depth += 1;
                self.push_frame(&frame_name);
                let saved = self.env.clone();
                let entry = clos.clone();
                let outcome = self.apply_closure_chain(clos, args.to_vec(), saved.clone());
                let exhausted = matches!(
                    &outcome,
                    Err(err) if err.code == "budget_exhausted"
                );
                self.env = saved;
                if !exhausted {
                    self.pop_frame();
                    self.call_depth = self.call_depth.saturating_sub(1);
                }
                let result = outcome?;
                self.remember_closure(&entry, args, result.clone());
                if args.len() > 1 {
                    self.apply_value(result, &args[1..])
                } else {
                    Ok(result)
                }
            }
            other => Err(fault(
                "type",
                format!("value is not callable: {other}"),
            )),
        }
    }

    /// The tail-call seam for CLOSURE applications: one pushed frame
    /// per application chain. A body whose tail position is a call to
    /// another closure value reuses the frame — the call depth stays
    /// at the entry value, and WORK is the only bound (the closure
    /// analogue of the named lane's frame-reuse loop). A tail call
    /// into a NAMED function delegates to that loop in this same
    /// frame. Non-tail shapes (a call nested inside an operation, a
    /// match arm, a sequence) never reach this loop: they evaluate
    /// through the ordinary nested-application path and keep the
    /// configured depth cap.
    pub(in crate::constructor_layer) fn apply_closure_chain(
        &mut self,
        mut clos: Box<Closure>,
        mut args: Vec<CValue>,
        saved: BTreeMap<String, CValue>,
    ) -> Result<CValue, ConstructorError> {
        loop {
            if args.is_empty() {
                return Ok(CValue::Closure(clos));
            }
            if let Some(value) = self.completed_closure(&clos, &args) {
                return Ok(value);
            }
            self.charge()?;
            let frame_name = clos
                .recursive
                .clone()
                .unwrap_or_else(|| format!("function {}", clos.param));
            // Frame reuse: this chain owns one frame; every iteration
            // rebinds it in place instead of pushing another.
            self.env = saved.clone();
            self.env.extend(clos.env.clone());
            if let Some(name) = &clos.recursive {
                self.env
                    .insert(name.clone(), CValue::Closure(clos.clone()));
            }
            if !clos.param.is_empty() {
                self.env.insert(clos.param.clone(), args[0].clone());
            }
            if let Some(frame) = self.frames.last_mut() {
                frame.function = frame_name;
                frame.env = self.env.clone();
                frame.kont.clear();
            }
            self.set_frame_next("body");
            self.push_kont(Kont::EvalExpr {
                expr: Rc::new(clos.body.clone()),
            });
            self.refresh_frame();
            let outcome = stacker::maybe_grow(1024 * 1024, 4 * 1024 * 1024, || {
                self.eval_tail(&clos.body)
            });
            match outcome {
                Ok(EvalTail::Value(value)) => {
                    self.pop_kont();
                    self.env = saved;
                    self.remember_closure(&clos, &args, value.clone());
                    return Ok(value);
                }
                Ok(EvalTail::Apply { callee, args: next_args }) => {
                    self.pop_kont();
                    match callee {
                        CValue::Closure(next) => {
                            clos = next;
                            args = next_args;
                            continue;
                        }
                        other => {
                            // Not a closure value after all: the
                            // ordinary nested-application path.
                            let value = self.apply_value(other, &next_args);
                            self.env = saved;
                            let value = value?;
                            self.remember_closure(&clos, &args, value.clone());
                            return Ok(value);
                        }
                    }
                }
                Ok(EvalTail::Call { name, args: vals }) => {
                    self.pop_kont();
                    let decl = self
                        .functions
                        .get(&name)
                        .cloned()
                        .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
                    // `apply_fn_body` trusts its caller for arity (it
                    // zip-binds inputs); the guard here keeps a
                    // wrong-arity tail call an `arity` fault, never a
                    // truncated binding faulting `unbound`.
                    let outcome = if decl.inputs.len() != vals.len() {
                        Err(fault(
                            "arity",
                            format!("expected {} arguments", decl.inputs.len()),
                        ))
                    } else {
                        self.apply_fn_body(&name, decl, &vals)
                    };
                    self.env = saved;
                    let value = outcome?;
                    self.remember_closure(&clos, &args, value.clone());
                    return Ok(value);
                }
                Err(err) => {
                    // The kont stays on any fault (resume bookkeeping
                    // matches the pre-seam application path, which
                    // popped only on success).
                    self.env = saved;
                    return Err(err);
                }
            }
        }
    }

    /// `quote.evaluate` (L1): guarded closed-code evaluation. Closed
    /// code evaluates as before; OPEN code — free names that are
    /// neither quoted dependencies nor current callables — refuses
    /// `unbound_code` instead of silently resolving against the ambient
    /// environment (dynamic-scope leak). Stamped dependencies that
    /// resolve differently refuse `stale_dependency`.
    pub(in crate::constructor_layer) fn eval_code(&mut self, value: CValue) -> Result<CValue, ConstructorError> {
        match value {
            CValue::Code(code) => {
                self.verify_dependencies(&code)?;
                let unbound = self.open_names(&code, &BTreeMap::new());
                if !unbound.is_empty() {
                    return Err(fault(
                        "unbound_code",
                        format!(
                            "quoted code is open; unbound name(s): {}",
                            unbound.join(", ")
                        ),
                    ));
                }
                self.eval(&code.expr)
            }
            other => Ok(other),
        }
    }

    /// Capture-time dependency snapshot: free callable names of the
    /// code body stamped with the identity of the declaration they
    /// resolve against at quote time.
    pub(in crate::constructor_layer) fn dependency_snapshot(&self, expr: &Expr) -> BTreeMap<String, u64> {
        let mut free = BTreeSet::new();
        free_path_names(expr, &mut Vec::new(), &mut free);
        let mut deps = BTreeMap::new();
        for name in free {
            if let Some(decl) = self.functions.get(&name) {
                deps.insert(name, decl_stamp(decl));
            }
        }
        deps
    }

    /// A stamped dependency that no longer resolves, or resolves to a
    /// different declaration, refuses instead of re-resolving.
    pub(in crate::constructor_layer) fn verify_dependencies(&self, code: &Code) -> Result<(), ConstructorError> {
        for (name, stamp) in &code.deps {
            let current = self.functions.get(name).map(|decl| decl_stamp(decl));
            if current.as_ref() != Some(stamp) {
                return Err(fault(
                    "stale_dependency",
                    format!("quoted dependency `{name}` changed since capture"),
                ));
            }
        }
        Ok(())
    }

    /// Free names of the code body that no environment supplies: not
    /// bound internally, not a stamped dependency, not a current
    /// callable, not in the given input environment.
    pub(in crate::constructor_layer) fn open_names(&self, code: &Code, inputs: &BTreeMap<String, CValue>) -> Vec<String> {
        let mut free = BTreeSet::new();
        free_path_names(&code.expr, &mut Vec::new(), &mut free);
        free.into_iter()
            .filter(|name| {
                !code.deps.contains_key(name)
                    && !self.functions.contains_key(name)
                    && !inputs.contains_key(name)
            })
            .collect()
    }

    /// The L1 closed-code service: evaluate a quoted program with an
    /// EXPLICIT input environment. Free names resolve only from the
    /// input environment and pinned dependencies — never the ambient
    /// environment. A function-valued result consumes the ordered
    /// inputs not already consumed as environment bindings (the
    /// generic program-application seam).
    pub(in crate::constructor_layer) fn eval_closed_code(
        &mut self,
        code: &Code,
        inputs: &BTreeMap<String, CValue>,
        order: &[String],
    ) -> Result<CValue, ConstructorError> {
        self.verify_dependencies(code)?;
        let unbound = self.open_names(code, inputs);
        if !unbound.is_empty() {
            return Err(fault(
                "unbound_code",
                format!(
                    "quoted code is open; unbound name(s): {}",
                    unbound.join(", ")
                ),
            ));
        }
        let mut free = BTreeSet::new();
        free_path_names(&code.expr, &mut Vec::new(), &mut free);
        let saved = self.env.clone();
        self.env = inputs.clone();
        let outcome = self.eval(&code.expr);
        self.env = saved;
        let value = outcome?;
        if matches!(value, CValue::Closure(_)) {
            let mut acc = value;
            for name in order {
                if free.contains(name) {
                    continue;
                }
                if let Some(arg) = inputs.get(name) {
                    acc = self.apply_value(acc, &[arg.clone()])?;
                }
            }
            return Ok(acc);
        }
        Ok(value)
    }

    pub(in crate::constructor_layer) fn quote_body(&self, value: CValue) -> Result<CValue, ConstructorError> {
        let CValue::Code(code) = value else {
            return Ok(body_record("Opaque", None, None));
        };
        if let ExprKind::Path { segments, .. } = &code.expr.kind {
            let name = segments.join(".");
            if let Some(decl) = self.functions.get(&name) {
                if decl.opaque {
                    return Ok(body_record("Opaque", Some(&name), Some("opaque")));
                }
                return Ok(available_body(function_body_expr(decl)));
            }
            return Ok(body_record("Opaque", Some(&name), Some("unbound")));
        }
        Ok(available_body(code.expr.clone()))
    }

    pub(in crate::constructor_layer) fn quote_make(&mut self, node: CValue) -> Result<CValue, ConstructorError> {
        match node {
            CValue::Code(code) => Ok(CValue::Code(code)),
            CValue::Record { type_name, fields } if type_name == "Fragment" => {
                match fields.get("term") {
                    Some(CValue::Code(code)) => Ok(CValue::Code(code.clone())),
                    Some(other) => match rebuild_expr(other) {
                        Ok(expr) => {
                            let deps = self.dependency_snapshot(&expr);
                            Ok(CValue::Code(Box::new(Code { expr, deps })))
                        }
                        Err(message) => Err(fault("invalid_code_construction", message)),
                    },
                    None => Err(fault("type", "Fragment missing term")),
                }
            }
            other => match rebuild_expr(&other) {
                Ok(expr) => {
                    let deps = self.dependency_snapshot(&expr);
                    Ok(CValue::Code(Box::new(Code { expr, deps })))
                }
                Err(message) => Err(fault("invalid_code_construction", message)),
            },
        }
    }

    pub(in crate::constructor_layer) fn check_fragment_scope(&self, package: &CValue) -> Result<(), ConstructorError> {
        let CValue::Record { type_name, fields } = package else {
            return Ok(());
        };
        if type_name != "Fragment" {
            return Ok(());
        }
        if let Some(CValue::Record {
            type_name,
            fields: scope,
        }) = fields.get("context")
        {
            if type_name == "Scope" {
                if let Some(CValue::Int(id)) = scope.get("id") {
                    let Some(id) = id.to_i128() else {
                        return Err(fault("invalid_code_construction", "forged Scope witness"));
                    };
                    if id < 0 || !self.scopes.contains(&(id as u64)) {
                        return Err(fault(
                            "invalid_code_construction",
                            "forged Scope witness",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(in crate::constructor_layer) fn quote_view(&mut self, value: CValue) -> Result<CValue, ConstructorError> {
        match fragment_term(value) {
            Ok(expr) => Ok(view_of(
                &expr,
                &self.functions,
                &mut self.next_ref,
                &mut self.scopes,
            )),
            Err(message) => Err(fault("type", message)),
        }
    }

}

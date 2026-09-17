use super::super::*;
use super::prelude::{admit_input_type, code_identity, is_refused_recipe, project_field, rebuild_expr};

impl Engine {
    pub(in crate::constructor_layer) fn eval_call(&mut self, function: &Expr, args: &[Expr]) -> Result<CValue, ConstructorError> {
        if let ExprKind::Path { segments, .. } = &function.kind {
            if let Some(value) = self.eval_named_call(segments, args)? {
                return Ok(value);
            }
        }
        let cal = self.eval(function)?;
        let mut vals = Vec::new();
        let mut rest: Vec<Expr> = args.to_vec();
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::CallArgs {
                callee: cal.clone(),
                done: vals.clone(),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Box::new(next.clone()),
            });
            vals.push(self.eval(&next)?);
            self.pop_kont();
            self.pop_kont();
        }
        self.apply_value(cal, &vals)
    }

    pub(in crate::constructor_layer) fn eval_named_call(
        &mut self,
        segments: &[String],
        args: &[Expr],
    ) -> Result<Option<CValue>, ConstructorError> {
        let name = segments.join(".");
        if name.starts_with("quote.") {
            return self.eval_quote_call(&name, args).map(Some);
        }
        if segments.len() == 2 && segments[1] == "pack" {
            return self.pack_object(&segments[0], args).map(Some);
        }
        if name == "transformation_rule_unavailable" || name.ends_with(".opaque") {
            return Err(fault(
                "transformation_rule_unavailable",
                "opaque operation has no exposed transformation rule",
            ));
        }
        if name == "constructor_refuse" {
            return self.eval_constructor_refuse(args).map(Some);
        }
        if let Some(op) = machine_int_basename(&name) {
            return self.eval_machine_int(op, args).map(Some);
        }
        if let Some(decl) = self.functions.get(&name).cloned() {
            return self.eval_fn(&name, &decl, args).map(Some);
        }
        // A name the user bound (env) is the user's; the recipe refusal is
        // for UNBOUND names, so a user closure named like a module method
        // still resolves through the ordinary callee evaluation below.
        if is_refused_recipe(&name) && !self.env.contains_key(&name) {
            return Err(fault(
                "method_unavailable",
                format!("`{name}` is an ordinary module method, not a constructor operation"),
            ));
        }
        if name == "length" {
            return match self.eval(&args[0])? {
                CValue::Sequence(xs) => Ok(Some(cint(xs.len()))),
                CValue::Tuple(xs) => Ok(Some(cint(xs.len()))),
                _ => Err(fault("type", "length expects a sequence")),
            };
        }
        if segments.len() == 1 && args.len() == 1 {
            let recv = self.eval(&args[0])?;
            if let Some(field) = project_field(&recv, &segments[0]) {
                return Ok(Some(field));
            }
        }
        Ok(None)
    }

    pub(in crate::constructor_layer) fn eval_quote_call(&mut self, name: &str, args: &[Expr]) -> Result<CValue, ConstructorError> {
        match name {
            "quote.evaluate" => {
                let code = self.eval(&args[0])?;
                self.eval_code(code)
            }
            "quote.identity" => {
                let code = self.eval(&args[0])?;
                let CValue::Code(code) = code else {
                    return Err(fault("type", "quote.identity expects code"));
                };
                Ok(cint(code_identity(&code.expr) as i64))
            }
            "quote.body" => {
                let code = self.eval(&args[0])?;
                self.quote_body(code)
            }
            "quote.make" => {
                if args.is_empty() {
                    return Err(fault("arity", "quote.make expects a node"));
                }
                let node = self.eval(&args[0])?;
                self.quote_make(node)
            }
            "quote.view" => {
                let code = self.eval(&args[0])?;
                self.quote_view(code)
            }
            "quote.substitute" => {
                if args.len() < 3 {
                    return Err(fault("arity", "quote.substitute expects fragment, reference, replacement"));
                }
                let fragment = self.eval(&args[0])?;
                let reference = match &args[1].kind {
                    ExprKind::Str(name) => name.clone(),
                    ExprKind::Path { segments, .. } if segments.len() == 1 => segments[0].clone(),
                    _ => match self.eval(&args[1])? {
                        CValue::Record { type_name, .. } => type_name,
                        CValue::Code(code) => match &code.expr.kind {
                            ExprKind::Path { segments, .. } => segments.join("."),
                            _ => {
                                return Err(fault(
                                    "type",
                                    "quote.substitute reference must name a binder",
                                ));
                            }
                        },
                        other => {
                            return Err(fault(
                                "type",
                                format!("quote.substitute reference must name a binder, found {other}"),
                            ));
                        }
                    },
                };
                let replacement = self.eval(&args[2])?;
                self.quote_substitute(fragment, &reference, replacement)
            }
            "quote.bind" => {
                if args.is_empty() {
                    return Err(fault("arity", "quote.bind expects a body"));
                }
                let body = match self.eval(&args[0])? {
                    CValue::Code(code) => code.expr,
                    other => rebuild_expr(&other).map_err(|msg| {
                        fault("invalid_code_construction", msg)
                    })?,
                };
                let expr = self.mint_binds(&body);
                let deps = self.dependency_snapshot(&expr);
                Ok(CValue::Code(Box::new(Code { expr, deps })))
            }
            other => Err(fault("unbound", format!("unknown quote operation `{other}`"))),
        }
    }

    /// `constructor_refuse(quote(reason))`: the generic intentional
    /// refusal ABI. The argument must evaluate to `Code` wrapping a
    /// single path identifier; the refusal is the named fault
    /// `mathematical method refused: {reason}` under that reason code.
    pub(in crate::constructor_layer) fn eval_constructor_refuse(&mut self, args: &[Expr]) -> Result<CValue, ConstructorError> {
        if args.len() != 1 {
            return Err(fault("arity", "constructor_refuse expects one argument"));
        }
        let reason = match self.eval(&args[0])? {
            CValue::Code(code) => match &code.expr.kind {
                ExprKind::Path { segments, .. } if segments.len() == 1 => segments[0].clone(),
                _ => {
                    return Err(fault(
                        "type",
                        "constructor_refuse expects a quoted single identifier",
                    ));
                }
            },
            other => {
                return Err(fault(
                    "type",
                    format!("constructor_refuse expects a quoted reason, found {other}"),
                ));
            }
        };
        Err(fault(
            &reason,
            format!("mathematical method refused: {reason}"),
        ))
    }

    pub(in crate::constructor_layer) fn eval_fn(&mut self, name: &str, decl: &FnDecl, args: &[Expr]) -> Result<CValue, ConstructorError> {
        if decl.inputs.len() != args.len() {
            return Err(fault(
                "arity",
                format!("expected {} arguments", decl.inputs.len()),
            ));
        }
        // Redline 1 MiB: one authored call costs ~50-100 KiB of NATIVE
        // stack (K-machine frames between growth points), so a 64 KiB
        // redline lets a level jump past it into the guard page before
        // the next check — a library caller on a default-sized thread
        // crashed at ~40 levels. Growth engages with ten levels of
        // headroom instead.
        stacker::maybe_grow(1024 * 1024, 4 * 1024 * 1024, || {
            self.eval_fn_inner(name, decl, args)
        })
    }

    pub(in crate::constructor_layer) fn eval_fn_inner(
        &mut self,
        name: &str,
        decl: &FnDecl,
        args: &[Expr],
    ) -> Result<CValue, ConstructorError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(self.recursion_depth_exceeded(name));
        }
        self.call_depth += 1;
        self.push_frame(name);
        let saved = self.env.clone();
        let result = self.eval_fn_args(name, decl, Vec::new(), args.to_vec());
        let exhausted = matches!(
            &result,
            Err(err) if err.code == "budget_exhausted"
        );
        self.env = saved;
        if !exhausted {
            self.pop_frame();
            self.call_depth = self.call_depth.saturating_sub(1);
        }
        result
    }

    pub(in crate::constructor_layer) fn eval_fn_args(
        &mut self,
        name: &str,
        decl: &FnDecl,
        mut done: Vec<CValue>,
        mut rest: Vec<Expr>,
    ) -> Result<CValue, ConstructorError> {
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::FnCall {
                name: name.to_string(),
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
        self.apply_fn_body(name, decl, &done)
    }

    pub(in crate::constructor_layer) fn finish_fn_call(
        &mut self,
        name: String,
        mut done: Vec<CValue>,
        rest: Vec<Expr>,
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        let decl = self
            .functions
            .get(&name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
        self.eval_fn_args(&name, &decl, done, rest)
    }

    pub(in crate::constructor_layer) fn finish_call_args(
        &mut self,
        callee: CValue,
        mut done: Vec<CValue>,
        mut rest: Vec<Expr>,
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::CallArgs {
                callee: callee.clone(),
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
        self.apply_value(callee, &done)
    }

    pub(in crate::constructor_layer) fn apply_fn_body(
        &mut self,
        name: &str,
        decl: &FnDecl,
        vals: &[CValue],
    ) -> Result<CValue, ConstructorError> {
        let mut name = name.to_string();
        let mut decl = decl.clone();
        let mut vals = vals.to_vec();
        'tco: loop {
            if let Some(value) = self.completed_call(&name, &vals) {
                return Ok(value);
            }
            for (ty, value) in decl.input_types.iter().zip(vals.iter()) {
                admit_input_type(ty, value)?;
            }
            self.env.clear();
            for (input, value) in decl.inputs.iter().zip(vals.iter()) {
                self.env.insert(input.clone(), value.clone());
            }
            if let Some(frame) = self.frames.last_mut() {
                frame.function = name.clone();
                frame.env = self.env.clone();
                frame.kont.clear();
            }
            self.charge()?;
            self.refresh_frame();
            let mut last = CValue::Unit;
            if decl.defs.is_empty() {
                last = self.function_result(&decl, &name, last)?;
                self.set_frame_next("__done");
                self.remember_call(&name, &vals, last.clone());
                return Ok(last);
            }
            for (index, (dname, expr)) in decl.defs.iter().enumerate() {
                self.set_frame_next(dname);
                if index + 1 == decl.defs.len() {
                    if decl.outputs.len() > 1 {
                        // Multi-output functions pack a result record from
                        // every output binding once the last definition has
                        // run. A bare named call in that position is not a
                        // tail call: TCO would return the callee's value and
                        // skip `function_result`'s record packing, leaving
                        // callers unable to project output fields.
                        let value = self.eval(expr)?;
                        self.env.insert(dname.clone(), value.clone());
                        last = self.function_result(&decl, &name, value)?;
                        self.set_frame_next("__done");
                        self.remember_call(&name, &vals, last.clone());
                        return Ok(last);
                    }
                    match self.eval_tail(expr)? {
                        EvalTail::Value(value) => {
                            self.env.insert(dname.clone(), value.clone());
                            last = self.function_result(&decl, &name, value)?;
                            self.set_frame_next("__done");
                            self.remember_call(&name, &vals, last.clone());
                            return Ok(last);
                        }
                        EvalTail::Call {
                            name: next_name,
                            args,
                        } => {
                            let next_decl = self.functions.get(&next_name).cloned().ok_or_else(
                                || fault("unbound", format!("unknown function `{next_name}`")),
                            )?;
                            name = next_name;
                            decl = next_decl;
                            vals = args;
                            continue 'tco;
                        }
                    }
                } else {
                    last = self.eval(expr)?;
                    self.env.insert(dname.clone(), last.clone());
                }
            }
        }
    }

}

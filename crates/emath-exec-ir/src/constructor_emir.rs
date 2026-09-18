//! Lower constructor-layer functions onto generic EMIR.
//!
//! Scalar operators stay representation ops. Executable quote transforms
//! are eliminated to residual scalar expressions before lowering. Opaque
//! or missing transformation rules stay unresolved and are not marked
//! runnable.

use emath_core::Span;
use emath_core::tree::{
    BinaryOp, Declaration, Expr, ExprKind, Item, StmtKind, SyntaxTree, UnaryOp,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::{DomainObligation, EmirOp, EmirProgram, EmirValue};
use crate::constructor_layer::{machine_buffer_basename, machine_int_basename};
use crate::exact_int::ExactInt;

#[derive(Clone, Debug, PartialEq)]
pub struct LoweredFunction {
    pub program: EmirProgram,
    pub inputs: Vec<String>,
    pub runnable: bool,
    pub unresolved: Vec<String>,
}

pub fn lower_constructor_function(
    tree: &SyntaxTree,
    name: &str,
) -> Result<LoweredFunction, String> {
    let mut cache = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    lower_named(tree, name, &mut cache, &mut visiting)
}

fn lower_named(
    tree: &SyntaxTree,
    name: &str,
    cache: &mut BTreeMap<String, LoweredFunction>,
    visiting: &mut BTreeSet<String>,
) -> Result<LoweredFunction, String> {
    if let Some(hit) = cache.get(name) {
        return Ok(hit.clone());
    }
    if !visiting.insert(name.to_string()) {
        return Err(format!("recursive sibling `{name}` is not emitted"));
    }
    let decl = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(decl) if decl.as_kind == "function" && decl.name == name => {
                Some(decl)
            }
            _ => None,
        })
        .ok_or_else(|| format!("unknown function `{name}`"))?;
    let inputs = section_fields(decl, "inputs");
    let outputs = section_fields(decl, "outputs");
    let authored_defs = section_assigns(decl, "definitions");
    let defs = if authored_defs.iter().any(|(_, expr)| expr_uses_quote(expr)) {
        match crate::constructor_layer::residual_output_exprs(tree, name) {
            Ok(residual) => residual,
            Err(_) => authored_defs,
        }
    } else {
        authored_defs
    };
    let mut unresolved = Vec::new();
    for (_, expr) in &defs {
        collect_unresolved(expr, &mut unresolved);
    }
    let mut siblings = BTreeMap::new();
    let mut callees = BTreeSet::new();
    for (_, expr) in &defs {
        collect_called_names(expr, &mut callees);
    }
    for callee in callees {
        if callee == name || !function_exists(tree, &callee) {
            continue;
        }
        match lower_named(tree, &callee, cache, visiting) {
            Ok(other) if other.runnable => {
                siblings.insert(callee, other.program);
            }
            Ok(other) => {
                unresolved.extend(other.unresolved);
                unresolved.push(callee);
            }
            Err(err) => unresolved.push(err),
        }
    }
    unresolved.sort();
    unresolved.dedup();
    let mut lowerer = Lowerer {
        inputs: inputs.clone(),
        locals: BTreeMap::new(),
        siblings,
        objects: object_kinds(tree),
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name: Some(name.to_string()),
    };
    if !unresolved.is_empty() {
        visiting.remove(name);
        let result = lowerer.push(EmirOp::Refuse(format!(
            "unresolved symbolic code: {}",
            unresolved.join(", ")
        )));
        let lowered = LoweredFunction {
            program: lowerer.finish(result),
            inputs,
            runnable: false,
            unresolved,
        };
        cache.insert(name.to_string(), lowered.clone());
        return Ok(lowered);
    }
    for (def_name, expr) in &defs {
        match lowerer.expr(expr) {
            Ok(value) => {
                lowerer.locals.insert(def_name.clone(), value);
            }
            Err(err) => {
                visiting.remove(name);
                unresolved.push(err);
                let result = lowerer.push(EmirOp::Refuse(format!(
                    "unresolved symbolic code: {}",
                    unresolved.join(", ")
                )));
                let lowered = LoweredFunction {
                    program: lowerer.finish(result),
                    inputs,
                    runnable: false,
                    unresolved,
                };
                cache.insert(name.to_string(), lowered.clone());
                return Ok(lowered);
            }
        }
    }
    let result = if outputs.len() > 1 {
        let mut fields = Vec::new();
        for output in &outputs {
            let Some(value) = lowerer.locals.get(output).copied() else {
                visiting.remove(name);
                return Err(format!("output `{output}` has no definition"));
            };
            fields.push((output.clone(), value));
        }
        lowerer.push(EmirOp::RecordCreate {
            type_name: name.to_string(),
            fields,
        })
    } else {
        let output = outputs
            .first()
            .cloned()
            .or_else(|| defs.last().map(|(def_name, _)| def_name.clone()))
            .ok_or_else(|| format!("function `{name}` has no output"))?;
        lowerer
            .locals
            .get(&output)
            .copied()
            .ok_or_else(|| format!("function `{name}` has no definition"))?
    };
    visiting.remove(name);
    let lowered = LoweredFunction {
        program: lowerer.finish(result),
        inputs,
        runnable: true,
        unresolved,
    };
    cache.insert(name.to_string(), lowered.clone());
    Ok(lowered)
}

fn function_exists(tree: &SyntaxTree, name: &str) -> bool {
    tree.items.iter().any(|item| {
        matches!(
            item,
            Item::Declaration(decl) if decl.as_kind == "function" && decl.name == name
        )
    })
}

struct Lowerer {
    inputs: Vec<String>,
    locals: BTreeMap<String, EmirValue>,
    siblings: BTreeMap<String, EmirProgram>,
    objects: BTreeMap<String, String>,
    ops: Vec<(EmirOp, Span)>,
    obligations: Vec<DomainObligation>,
    self_name: Option<String>,
}

impl Lowerer {
    fn push(&mut self, op: EmirOp) -> EmirValue {
        let index = u32::try_from(self.ops.len()).expect("emir op count");
        self.ops.push((op, Span::default()));
        EmirValue(index)
    }

    fn current_inputs(&mut self) -> Vec<EmirValue> {
        (0..self.inputs.len())
            .map(|index| self.push(EmirOp::LoadInput(index as u16)))
            .collect()
    }

    fn nested(&self, expr: &Expr) -> Result<EmirProgram, String> {
        let mut lowerer = self.capture_child();
        let result = lowerer.expr(expr)?;
        Ok(lowerer.finish(result))
    }

    fn captured_names(&self) -> Vec<String> {
        let mut names = self.inputs.clone();
        names.extend(self.locals.keys().cloned());
        names
    }

    fn captured_args(&mut self) -> Vec<EmirValue> {
        let mut args = self.current_inputs();
        for value in self.locals.values() {
            args.push(*value);
        }
        args
    }

    fn capture_child(&self) -> Lowerer {
        Lowerer {
            inputs: self.captured_names(),
            locals: BTreeMap::new(),
            siblings: self.siblings.clone(),
            objects: self.objects.clone(),
            ops: Vec::new(),
            obligations: Vec::new(),
            self_name: self.self_name.clone(),
        }
    }

    fn lower_recur_apply(
        &mut self,
        name: &str,
        body: &Expr,
        args: &[Expr],
    ) -> Result<EmirValue, String> {
        let (param, inner) = match &body.kind {
            ExprKind::FunctionAbs { param, body, .. } => (param.as_str(), body.as_ref()),
            _ => return Err("recur emission expects a function literal".into()),
        };
        let program = lower_int_primitive_recur(name, param, inner)
            .map_or_else(|| lower_general_recur(name, param, inner), Ok)?;
        let mut inputs = Vec::new();
        for arg in args {
            inputs.push(self.expr(arg)?);
        }
        Ok(self.push(EmirOp::CallFrame {
            body: program,
            inputs,
            state: Vec::new(),
        }))
    }

    fn lower_cases(
        &mut self,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
    ) -> Result<EmirValue, String> {
        let Some((condition, then_value)) = arms.first() else {
            return self.expr(else_arm);
        };
        let condition = self.expr(condition)?;
        let then_body = self.nested(then_value)?;
        let else_body = {
            let mut inner = self.capture_child();
            let result = inner.lower_cases(&arms[1..], else_arm)?;
            inner.finish(result)
        };
        let args = self.captured_args();
        Ok(self.push(EmirOp::Branch {
            condition,
            args,
            then_body,
            else_body,
        }))
    }

    fn lookup_path(&mut self, segments: &[String]) -> Result<EmirValue, String> {
        let name = segments.join(".");
        if let Some(value) = self.locals.get(&name) {
            return Ok(*value);
        }
        if let Some(index) = self.inputs.iter().position(|input| input == &name) {
            return Ok(self.push(EmirOp::LoadInput(index as u16)));
        }
        if segments.len() == 2 {
            let record = self.lookup_path(&segments[..1])?;
            return Ok(self.push(EmirOp::RecordField {
                record,
                field: segments[1].clone(),
            }));
        }
        Err(format!("unbound `{name}` in emission"))
    }

    fn lower_named_call(&mut self, segments: &[String], args: &[Expr]) -> Result<EmirValue, String> {
        let called = segments.join(".");
        if called == "length" && args.len() == 1 {
            let value = self.expr(&args[0])?;
            return Ok(self.push(EmirOp::VectorLength(value)));
        }
        // Fence (bead emath-84sfr, design note 12 B1): buffer-carrier
        // ops are constructor-VM machine seams, not emitted math. The
        // named refusal keeps emission honest instead of emitting a
        // call the artifact cannot honor.
        if machine_buffer_basename(&called).is_some() {
            return Err(
                "buffer carrier ops run in the constructor VM; they are not emitted in this cut (bead emath-84sfr fence)"
                    .into(),
            );
        }
        if let Some(op) = machine_int_basename(&called) {
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::ExactIntCall {
                name: op.to_string(),
                args: inputs,
            }));
        }
        if called == "constructor_refuse" {
            return Err(
                "constructor_refuse is an intentional refusal ABI; it is not emitted as a successful call"
                    .into(),
            );
        }
        if called.starts_with("quote.") {
            return Err(format!("call is not yet emitted: {called}"));
        }
        if segments.len() == 2 && segments[1] == "pack" {
            return self.lower_pack(&segments[0], args);
        }
        if segments.len() == 2 && segments[1] == "open" {
            return Err("open is a binder, not a plain call".into());
        }
        if self.self_name.as_deref() == Some(called.as_str()) {
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::CallSelf { inputs }));
        }
        if let Some(body) = self.siblings.get(&called).cloned() {
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::CallFrame {
                body,
                inputs,
                state: Vec::new(),
            }));
        }
        if segments.len() == 1 && args.len() == 1 && !self.inputs.iter().any(|input| input == &called)
        {
            let record = self.expr(&args[0])?;
            return Ok(self.push(EmirOp::RecordField {
                record,
                field: called,
            }));
        }
        Err(format!("call is not yet emitted: {called}"))
    }

    fn lower_pack(&mut self, type_name: &str, args: &[Expr]) -> Result<EmirValue, String> {
        let kind = self
            .objects
            .get(type_name)
            .cloned()
            .ok_or_else(|| format!("unknown object `{type_name}`"))?;
        match kind.as_str() {
            "abstract" => {
                if args.len() != 1 {
                    return Err("abstract pack expects one representation".into());
                }
                let repr = self.expr(&args[0])?;
                Ok(self.push(EmirOp::RecordCreate {
                    type_name: type_name.to_string(),
                    fields: vec![("repr".into(), repr)],
                }))
            }
            "package" => {
                if args.len() < 2 {
                    return Err("package pack expects n and data".into());
                }
                if let (Some(want), Some(got)) = (static_int(&args[0]), static_list_len(&args[1])) {
                    if want != got {
                        return Err(format!(
                            "invalid_index: T[n] pack length {got} != {want}"
                        ));
                    }
                }
                let n = self.expr(&args[0])?;
                let data = self.expr(&args[1])?;
                Ok(self.push(EmirOp::RecordCreate {
                    type_name: type_name.to_string(),
                    fields: vec![("n".into(), n), ("data".into(), data)],
                }))
            }
            other => Err(format!(
                "`{type_name}.pack` requires abstract or package representation, found {other}"
            )),
        }
    }

    fn lower_open(
        &mut self,
        type_name: &str,
        param: &str,
        packed: &Expr,
        body: &Expr,
    ) -> Result<EmirValue, String> {
        let kind = self
            .objects
            .get(type_name)
            .cloned()
            .ok_or_else(|| format!("unknown object `{type_name}`"))?;
        let packed = self.expr(packed)?;
        let opened = if kind == "abstract" {
            self.push(EmirOp::RecordField {
                record: packed,
                field: "repr".into(),
            })
        } else {
            packed
        };
        let previous = self.locals.insert(param.to_string(), opened);
        let result = self.expr(body);
        match previous {
            Some(value) => {
                self.locals.insert(param.to_string(), value);
            }
            None => {
                self.locals.remove(param);
            }
        }
        result
    }

    fn finish(self, result: EmirValue) -> EmirProgram {
        EmirProgram {
            ops: self.ops,
            result,
            input_count: u16::try_from(self.inputs.len()).expect("input count"),
            state_count: 0,
            domain_obligations: self.obligations,
        }
    }

    fn expr(&mut self, expr: &Expr) -> Result<EmirValue, String> {
        match &expr.kind {
            ExprKind::Int(text) => Ok(self.push(const_exact_int(text)?)),
            ExprKind::Bool(value) => Ok(self.push(EmirOp::ConstBool(*value))),
            ExprKind::Rational { numer, denom } => {
                let num = self.push(const_exact_int(numer)?);
                let den = self.push(const_exact_int(denom)?);
                self.obligations.push(DomainObligation::DivisionNonZero);
                Ok(self.push(EmirOp::F64Div(num, den)))
            }
            ExprKind::Path { segments, .. } => self.lookup_path(segments),
            ExprKind::Unary { op, value } => {
                let value = self.expr(value)?;
                match op {
                    UnaryOp::Neg => Ok(self.push(EmirOp::Neg(value))),
                    UnaryOp::Not => Ok(self.push(EmirOp::Not(value))),
                    UnaryOp::Pos => Ok(value),
                }
            }
            ExprKind::Binary { op, left, right } => {
                let left = self.expr(left)?;
                let right = self.expr(right)?;
                let opcode = match op {
                    BinaryOp::Add => EmirOp::F64Add(left, right),
                    BinaryOp::Sub => EmirOp::F64Sub(left, right),
                    BinaryOp::Mul => EmirOp::F64Mul(left, right),
                    BinaryOp::Div => {
                        self.obligations.push(DomainObligation::DivisionNonZero);
                        EmirOp::F64Div(left, right)
                    }
                    BinaryOp::Eq => EmirOp::Eq(left, right),
                    BinaryOp::Ne => EmirOp::Ne(left, right),
                    BinaryOp::Lt => EmirOp::Lt(left, right),
                    BinaryOp::Le => EmirOp::Le(left, right),
                    BinaryOp::Gt => EmirOp::Gt(left, right),
                    BinaryOp::Ge => EmirOp::Ge(left, right),
                    BinaryOp::And => EmirOp::And(left, right),
                    BinaryOp::Or => EmirOp::Or(left, right),
                    other => {
                        return Err(format!(
                            "operator {other:?} is not a scalar carrier operation"
                        ));
                    }
                };
                Ok(self.push(opcode))
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expr(condition)?;
                let then_body = self.nested(then_value)?;
                let else_body = self.nested(else_value)?;
                let args = self.captured_args();
                Ok(self.push(EmirOp::Branch {
                    condition,
                    args,
                    then_body,
                    else_body,
                }))
            }
            ExprKind::Call { function, args } => match &function.kind {
                ExprKind::Recur { name, body, .. } => self.lower_recur_apply(name, body, args),
                ExprKind::FunctionAbs { param, body, .. } => {
                    let nested = lower_closed(std::slice::from_ref(param), body, None)?;
                    let mut inputs = Vec::new();
                    for arg in args {
                        inputs.push(self.expr(arg)?);
                    }
                    Ok(self.push(EmirOp::CallFrame {
                        body: nested,
                        inputs,
                        state: Vec::new(),
                    }))
                }
                ExprKind::Path { segments, .. } => self.lower_named_call(segments, args),
                other => Err(format!("call is not yet emitted: {other:?}")),
            },
            ExprKind::FunctionAbs { param, body, .. } => {
                let nested = lower_closed(std::slice::from_ref(param), body, None)?;
                Ok(self.push(EmirOp::program_literal(nested)))
            }
            ExprKind::List(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.push(self.expr(item)?);
                }
                Ok(self.push(EmirOp::ListCreate(values)))
            }
            ExprKind::Tuple(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.push(self.expr(item)?);
                }
                Ok(self.push(EmirOp::ListCreate(values)))
            }
            ExprKind::Index { value, indices } => {
                let vector = self.expr(value)?;
                let Some(index) = indices.first() else {
                    return Err("index requires at least one index".into());
                };
                let index = self.expr(index)?;
                Ok(self.push(EmirOp::VectorIndex { vector, index }))
            }
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => match &callee.kind {
                ExprKind::Path { segments, .. }
                    if segments.len() == 2 && segments[1] == "open" =>
                {
                    self.lower_open(&segments[0], param, domain, body)
                }
                _ => Err(format!(
                    "expression form is not yet emitted: {:?}",
                    expr.kind
                )),
            },
            ExprKind::Cases { arms, else_arm, .. } => self.lower_cases(arms, else_arm),
            ExprKind::Recur { name, body, .. } => {
                if let ExprKind::FunctionAbs { param, body: inner, .. } = &body.kind {
                    if let Some(program) = lower_int_primitive_recur(name, param, inner) {
                        return Ok(self.push(EmirOp::program_literal(program)));
                    }
                    if contains_self_call(inner, name) {
                        let program = lower_general_recur(name, param, inner)?;
                        return Ok(self.push(EmirOp::program_literal(program)));
                    }
                }
                if contains_self_call(body, name) {
                    return Err(format!("recursive `{name}` is not a scalar emission yet"));
                }
                self.expr(body)
            }
            other => Err(format!(
                "expression form is not yet emitted: {other:?}"
            )),
        }
    }
}

fn lower_closed(
    inputs: &[String],
    body: &Expr,
    self_name: Option<String>,
) -> Result<EmirProgram, String> {
    let mut lowerer = Lowerer {
        inputs: inputs.to_vec(),
        locals: BTreeMap::new(),
        siblings: BTreeMap::new(),
        objects: BTreeMap::new(),
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name,
    };
    let result = lowerer.expr(body)?;
    Ok(lowerer.finish(result))
}

fn lower_general_recur(name: &str, param: &str, body: &Expr) -> Result<EmirProgram, String> {
    let inputs = vec![param.to_string()];
    lower_closed(&inputs, body, Some(name.to_string()))
}

fn expr_uses_quote(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Quote { .. } | ExprKind::QuoteBind { .. } => true,
        ExprKind::Call { function, args } => {
            if let ExprKind::Path { segments, .. } = &function.kind {
                if segments.first().map(String::as_str) == Some("quote") {
                    return true;
                }
            }
            expr_uses_quote(function) || args.iter().any(expr_uses_quote)
        }
        ExprKind::CallableBinder {
            callee,
            domain,
            body,
            ..
        } => {
            if let ExprKind::Path { segments, .. } = &callee.kind {
                if segments.first().map(String::as_str) == Some("quote") {
                    return true;
                }
            }
            expr_uses_quote(callee) || expr_uses_quote(domain) || expr_uses_quote(body)
        }
        ExprKind::Unary { value, .. } => expr_uses_quote(value),
        ExprKind::Binary { left, right, .. } => expr_uses_quote(left) || expr_uses_quote(right),
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            expr_uses_quote(condition)
                || expr_uses_quote(then_value)
                || expr_uses_quote(else_value)
        }
        ExprKind::FunctionAbs { body, domain, .. } | ExprKind::Recur { body, ty: domain, .. } => {
            expr_uses_quote(domain) || expr_uses_quote(body)
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            subject.as_ref().is_some_and(|subject| expr_uses_quote(subject))
                || arms.iter().any(|(cond, value)| {
                    expr_uses_quote(cond) || expr_uses_quote(value)
                })
                || expr_uses_quote(else_arm)
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => items.iter().any(expr_uses_quote),
        ExprKind::Index { value, indices } => {
            expr_uses_quote(value) || indices.iter().any(expr_uses_quote)
        }
        ExprKind::Record { fields, .. } => fields.iter().any(|(_, value)| expr_uses_quote(value)),
        _ => false,
    }
}

fn collect_unresolved(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Quote { .. } | ExprKind::QuoteBind { .. } => {
            out.push("quote".into());
        }
        ExprKind::Call { function, args } => {
            collect_unresolved(function, out);
            for arg in args {
                collect_unresolved(arg, out);
            }
        }
        ExprKind::Unary { value, .. } => collect_unresolved(value, out),
        ExprKind::Binary { left, right, .. } => {
            collect_unresolved(left, out);
            collect_unresolved(right, out);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_unresolved(condition, out);
            collect_unresolved(then_value, out);
            collect_unresolved(else_value, out);
        }
        ExprKind::FunctionAbs { body, domain, .. } => {
            collect_unresolved(domain, out);
            collect_unresolved(body, out);
        }
        ExprKind::Recur { body, ty, .. } => {
            collect_unresolved(ty, out);
            collect_unresolved(body, out);
        }
        ExprKind::CallableBinder {
            callee,
            domain,
            body,
            ..
        } => {
            if let ExprKind::Path { segments, .. } = &callee.kind {
                let name = segments.join(".");
                if name.starts_with("quote.") {
                    out.push("quote".into());
                }
            }
            collect_unresolved(callee, out);
            collect_unresolved(domain, out);
            collect_unresolved(body, out);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            if let Some(subject) = subject {
                collect_unresolved(subject, out);
            }
            for (cond, value) in arms {
                collect_unresolved(cond, out);
                collect_unresolved(value, out);
            }
            collect_unresolved(else_arm, out);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_unresolved(item, out);
            }
        }
        ExprKind::Index { value, indices } => {
            collect_unresolved(value, out);
            for index in indices {
                collect_unresolved(index, out);
            }
        }
        _ => {}
    }
}

fn collect_called_names(expr: &Expr, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Call { function, args } => {
            if let ExprKind::Path { segments, .. } = &function.kind {
                out.insert(segments.join("."));
            }
            collect_called_names(function, out);
            for arg in args {
                collect_called_names(arg, out);
            }
        }
        ExprKind::Unary { value, .. } => collect_called_names(value, out),
        ExprKind::Binary { left, right, .. } => {
            collect_called_names(left, out);
            collect_called_names(right, out);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_called_names(condition, out);
            collect_called_names(then_value, out);
            collect_called_names(else_value, out);
        }
        ExprKind::FunctionAbs { body, domain, .. } => {
            collect_called_names(domain, out);
            collect_called_names(body, out);
        }
        ExprKind::Recur { body, ty, .. } => {
            collect_called_names(ty, out);
            collect_called_names(body, out);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            if let Some(subject) = subject {
                collect_called_names(subject, out);
            }
            for (cond, value) in arms {
                collect_called_names(cond, out);
                collect_called_names(value, out);
            }
            collect_called_names(else_arm, out);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_called_names(item, out);
            }
        }
        ExprKind::Index { value, indices } => {
            collect_called_names(value, out);
            for index in indices {
                collect_called_names(index, out);
            }
        }
        _ => {}
    }
}

fn section_fields(decl: &Declaration, name: &str) -> Vec<String> {
    let mut names = Vec::new();
    for section in decl.sections().filter(|section| section.name == name) {
        for stmt in &section.suite.statements {
            if let StmtKind::FieldDecl { name, .. } = &stmt.kind {
                names.push(name.clone());
            }
        }
    }
    names
}

fn object_kinds(tree: &SyntaxTree) -> BTreeMap<String, String> {
    let mut kinds = BTreeMap::new();
    for item in &tree.items {
        if let Item::Declaration(decl) = item {
            if decl.as_kind == "object" {
                kinds.insert(decl.name.clone(), object_kind_of(decl));
            }
        }
    }
    kinds
}

fn object_kind_of(decl: &Declaration) -> String {
    for (name, expr) in section_assigns(decl, "representation") {
        if name == "kind" || name == "shape" {
            return match &expr.kind {
                ExprKind::Str(text) => text.clone(),
                ExprKind::Path { segments, .. } => {
                    segments.last().cloned().unwrap_or_else(|| "record".into())
                }
                _ => "record".into(),
            };
        }
    }
    "record".into()
}

fn static_int(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::Int(text) => text.replace('_', "").parse().ok(),
        _ => None,
    }
}

fn static_list_len(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::List(items) | ExprKind::Tuple(items) => i64::try_from(items.len()).ok(),
        _ => None,
    }
}

fn section_assigns(decl: &Declaration, name: &str) -> Vec<(String, Expr)> {
    let mut assigns = Vec::new();
    for section in decl.sections().filter(|section| section.name == name) {
        for stmt in &section.suite.statements {
            if let StmtKind::Assign { target, value } = &stmt.kind {
                if let Some(name) = target.segments.first() {
                    assigns.push((name.clone(), value.clone()));
                }
            }
        }
    }
    assigns
}

pub fn cvalue_to_emir(value: &crate::constructor_layer::CValue) -> Result<crate::interp::Value, String> {
    use crate::constructor_layer::CValue;
    use crate::interp::Value;
    match value {
        CValue::Bool(v) => Ok(Value::Bool(*v)),
        CValue::Int(v) => Ok(value_from_exact(v.clone())),
        CValue::Rat { num, den } => Ok(value_from_rat(num.clone(), den.clone())),
        CValue::Float64(v) => Ok(Value::F64(*v)),
        CValue::Sequence(items) => {
            let converted = items
                .iter()
                .map(cvalue_to_emir)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(converted))
        }
        CValue::Record { type_name, fields } => {
            let mut converted = BTreeMap::new();
            for (name, field) in fields {
                converted.insert(name.clone(), cvalue_to_emir(field)?);
            }
            Ok(Value::Record {
                type_name: type_name.clone(),
                fields: converted,
            })
        }
        CValue::Buffer(_) => Err(
            "buffer carrier values are not emitted in this cut (bead emath-84sfr fence); run in the constructor VM"
                .into(),
        ),
        other => Err(format!("no EMIR carrier for {other}")),
    }
}

pub fn values_equal(
    left: &crate::interp::Value,
    right: &crate::constructor_layer::CValue,
) -> Result<bool, String> {
    let converted = cvalue_to_emir(right)?;
    Ok(emir_values_equal(left, &converted))
}

fn value_from_exact(value: ExactInt) -> crate::interp::Value {
    match value.to_i64() {
        Some(n) => crate::interp::Value::I64(n),
        None => crate::interp::Value::ExactInt(value),
    }
}

fn value_from_rat(num: ExactInt, den: ExactInt) -> crate::interp::Value {
    match (num.to_i128(), den.to_i128()) {
        (Some(num), Some(den)) => crate::interp::Value::Rat { num, den },
        _ => crate::interp::Value::ExactRat { num, den },
    }
}

fn const_exact_int(text: &str) -> Result<EmirOp, String> {
    let cleaned = text.replace('_', "");
    if let Ok(value) = cleaned.parse::<i64>() {
        return Ok(EmirOp::ConstI64(value));
    }
    ExactInt::parse(&cleaned)
        .map(|_| EmirOp::ConstExactInt(cleaned))
        .map_err(|_| format!("not Int: {text}"))
}

fn emir_values_equal(left: &crate::interp::Value, right: &crate::interp::Value) -> bool {
    match (left, right) {
        (crate::interp::Value::ExactInt(a), crate::interp::Value::ExactInt(b)) => a == b,
        (crate::interp::Value::ExactInt(a), crate::interp::Value::I64(b)) => {
            a == &ExactInt::from(*b)
        }
        (crate::interp::Value::I64(a), crate::interp::Value::ExactInt(b)) => {
            ExactInt::from(*a) == *b
        }
        (
            crate::interp::Value::ExactRat { num: an, den: ad },
            crate::interp::Value::ExactRat { num: bn, den: bd },
        ) => an.mul(bd).ok().is_some_and(|left| {
            bn.mul(ad)
                .ok()
                .is_some_and(|right| left == right)
        }),
        (
            crate::interp::Value::ExactRat { num: an, den: ad },
            crate::interp::Value::Rat { num: bn, den: bd },
        )
        | (
            crate::interp::Value::Rat { num: bn, den: bd },
            crate::interp::Value::ExactRat { num: an, den: ad },
        ) => an
            .mul(&ExactInt::from(*bd))
            .ok()
            .is_some_and(|left| ExactInt::from(*bn).mul(ad).ok().is_some_and(|right| left == right)),
        (crate::interp::Value::Rat { num: a, den: ad }, crate::interp::Value::Rat { num: b, den: bd }) => {
            a * bd == b * ad
        }
        (crate::interp::Value::ExactInt(a), crate::interp::Value::Rat { num, den })
        | (crate::interp::Value::Rat { num, den }, crate::interp::Value::ExactInt(a)) => a
            .mul(&ExactInt::from(*den))
            .ok()
            .is_some_and(|left| left == ExactInt::from(*num)),
        (
            crate::interp::Value::ExactInt(a),
            crate::interp::Value::ExactRat { num, den },
        )
        | (
            crate::interp::Value::ExactRat { num, den },
            crate::interp::Value::ExactInt(a),
        ) => a
            .mul(den)
            .ok()
            .is_some_and(|left| left == *num),
        (crate::interp::Value::I64(a), crate::interp::Value::ExactRat { num, den })
        | (crate::interp::Value::ExactRat { num, den }, crate::interp::Value::I64(a)) => {
            ExactInt::from(*a)
                .mul(den)
                .ok()
                .is_some_and(|left| left == *num)
        }
        (crate::interp::Value::I64(a), crate::interp::Value::Rat { num, den }) => {
            i128::from(*a) * *den == *num
        }
        (crate::interp::Value::Rat { num, den }, crate::interp::Value::I64(b)) => {
            *num == i128::from(*b) * *den
        }
        (crate::interp::Value::List(a), crate::interp::Value::List(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(left_item, right_item)| emir_values_equal(left_item, right_item))
        }
        (
            crate::interp::Value::Record { fields: a, .. },
            crate::interp::Value::Record { fields: b, .. },
        ) => {
            a.len() == b.len()
                && a.iter().all(|(name, value)| {
                    b.get(name)
                        .is_some_and(|other| emir_values_equal(value, other))
                })
        }
        _ => left == right,
    }
}

fn lower_int_primitive_recur(name: &str, param: &str, body: &Expr) -> Option<EmirProgram> {
    let ExprKind::If {
        condition,
        then_value,
        else_value,
    } = &body.kind
    else {
        return None;
    };
    if contains_self_call(then_value, name) || !is_zero_test(condition, param) {
        return None;
    }
    let (op, coeff) = plus_self_pred(else_value, name, param)?;
    let mut step = Lowerer {
        inputs: vec!["__index".into(), "__acc".into(), param.to_string()],
        locals: BTreeMap::new(),
        siblings: BTreeMap::new(),
        objects: BTreeMap::new(),
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name: None,
    };
    let acc = step.push(EmirOp::LoadInput(1));
    let coeff = if contains_path(coeff, param) {
        let shift = dummy_emir_expr(ExprKind::Binary {
            op: BinaryOp::Sub,
            left: Box::new(path_emir(param)),
            right: Box::new(path_emir("__index")),
        });
        step.expr(&replace_path(coeff, param, &shift)).ok()?
    } else {
        step.expr(coeff).ok()?
    };
    let updated = step.push(match op {
        BinaryOp::Add => EmirOp::F64Add(acc, coeff),
        BinaryOp::Sub => EmirOp::F64Sub(acc, coeff),
        BinaryOp::Mul => EmirOp::F64Mul(acc, coeff),
        _ => return None,
    });
    let body = step.finish(updated);
    let mut outer = Lowerer {
        inputs: vec![param.to_string()],
        locals: BTreeMap::new(),
        siblings: BTreeMap::new(),
        objects: BTreeMap::new(),
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name: None,
    };
    let count = outer.push(EmirOp::LoadInput(0));
    let init = outer.expr(then_value).ok()?;
    let param_value = outer.push(EmirOp::LoadInput(0));
    let result = outer.push(EmirOp::Iterate {
        count,
        init,
        args: vec![param_value],
        stop: None,
        body,
    });
    Some(outer.finish(result))
}

fn dummy_emir_expr(kind: ExprKind) -> Expr {
    Expr {
        kind,
        source: Span::default(),
    }
}

fn path_emir(name: &str) -> Expr {
    dummy_emir_expr(ExprKind::Path {
        segments: vec![name.into()],
        generics: None,
    })
}

fn contains_path(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Path { segments, .. } => segments.join(".") == name,
        ExprKind::Binary { left, right, .. } => {
            contains_path(left, name) || contains_path(right, name)
        }
        ExprKind::Unary { value, .. } => contains_path(value, name),
        ExprKind::Call { function, args } => {
            contains_path(function, name) || args.iter().any(|arg| contains_path(arg, name))
        }
        _ => false,
    }
}

fn replace_path(expr: &Expr, name: &str, replacement: &Expr) -> Expr {
    if is_path(expr, name) {
        return replacement.clone();
    }
    let kind = match &expr.kind {
        ExprKind::Binary { op, left, right } => ExprKind::Binary {
            op: *op,
            left: Box::new(replace_path(left, name, replacement)),
            right: Box::new(replace_path(right, name, replacement)),
        },
        ExprKind::Unary { op, value } => ExprKind::Unary {
            op: *op,
            value: Box::new(replace_path(value, name, replacement)),
        },
        ExprKind::Call { function, args } => ExprKind::Call {
            function: Box::new(replace_path(function, name, replacement)),
            args: args
                .iter()
                .map(|arg| replace_path(arg, name, replacement))
                .collect(),
        },
        other => other.clone(),
    };
    Expr {
        kind,
        source: expr.source,
    }
}

fn is_path(expr: &Expr, name: &str) -> bool {
    matches!(&expr.kind, ExprKind::Path { segments, .. } if segments.join(".") == name)
}

fn is_int(expr: &Expr, want: i64) -> bool {
    matches!(&expr.kind, ExprKind::Int(text) if text.replace('_', "").parse::<i64>().ok() == Some(want))
}

fn is_zero_test(expr: &Expr, param: &str) -> bool {
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Eq | BinaryOp::Le,
            left,
            right,
        } => {
            (is_path(left, param) && is_int(right, 0)) || (is_path(right, param) && is_int(left, 0))
        }
        _ => false,
    }
}

fn is_pred_arg(expr: &Expr, param: &str) -> bool {
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Sub,
            left,
            right,
        } => is_path(left, param) && is_int(right, 1),
        _ => false,
    }
}

fn is_self_pred(expr: &Expr, name: &str, param: &str) -> bool {
    let ExprKind::Call { function, args } = &expr.kind else {
        return false;
    };
    let ExprKind::Path { segments, .. } = &function.kind else {
        return false;
    };
    segments.join(".") == name && args.len() == 1 && args.first().is_some_and(|arg| is_pred_arg(arg, param))
}

fn plus_self_pred<'a>(expr: &'a Expr, name: &str, param: &str) -> Option<(BinaryOp, &'a Expr)> {
    let ExprKind::Binary { op, left, right } = &expr.kind else {
        return None;
    };
    if !matches!(*op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
        return None;
    }
    if is_self_pred(left, name, param) && !contains_self_call(right, name) {
        return Some((*op, right));
    }
    if is_self_pred(right, name, param) && !contains_self_call(left, name) {
        return Some((*op, left));
    }
    None
}

fn contains_self_call(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Call { function, args } => {
            if let ExprKind::Path { segments, .. } = &function.kind {
                if segments.join(".") == name {
                    return true;
                }
            }
            contains_self_call(function, name) || args.iter().any(|arg| contains_self_call(arg, name))
        }
        ExprKind::FunctionAbs { body, domain, .. } => {
            contains_self_call(body, name) || contains_self_call(domain, name)
        }
        ExprKind::Recur { body, ty, .. } => {
            contains_self_call(body, name) || contains_self_call(ty, name)
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            contains_self_call(condition, name)
                || contains_self_call(then_value, name)
                || contains_self_call(else_value, name)
        }
        ExprKind::Binary { left, right, .. } => {
            contains_self_call(left, name) || contains_self_call(right, name)
        }
        ExprKind::Unary { value, .. } => contains_self_call(value, name),
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            items.iter().any(|item| contains_self_call(item, name))
        }
        ExprKind::Cases { subject, arms, else_arm } => {
            subject
                .as_ref()
                .is_some_and(|subject| contains_self_call(subject, name))
                || arms.iter().any(|(cond, value)| {
                    contains_self_call(cond, name) || contains_self_call(value, name)
                })
                || contains_self_call(else_arm, name)
        }
        _ => false,
    }
}


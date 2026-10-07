//! Constructor meaning over retained, scoped syntax as neutral semantic data.
//! Tags and length framing are explicit; spans and source rendering never enter.
//! Global graph traversal numbers nodes by first encounter, so recursion and
//! sharing retain topology without depending on declaration/import spellings.

use super::{Encoder, MeaningError};
use emath_core::tree::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct ConstructorSemantics {
    pub declarations: Vec<Declaration>,
    /// Engine-visible alias/qualified name -> scoped declaration key.
    pub aliases: BTreeMap<String, String>,
    /// Package declaration roots, in the package arena's order.
    pub roots: Vec<String>,
    /// Export visibility under scoped keys; object types are always global.
    pub exports: BTreeSet<String>,
}

impl ConstructorSemantics {
    fn names(&self) -> Result<BTreeMap<String, usize>, MeaningError> {
        let mut names = BTreeMap::new();
        for (index, decl) in self.declarations.iter().enumerate() {
            if names.insert(decl.name.clone(), index).is_some() {
                return Err(MeaningError::InvalidConstructor(format!(
                    "duplicate graph key `{}`",
                    decl.name
                )));
            }
        }
        for key in self
            .aliases
            .values()
            .chain(self.roots.iter())
            .chain(self.exports.iter())
        {
            if !names.contains_key(key) {
                return Err(MeaningError::InvalidConstructor(format!(
                    "missing graph key `{key}`"
                )));
            }
        }
        Ok(names)
    }

    pub(super) fn root_bytes(&self, root: &str) -> Result<Vec<u8>, MeaningError> {
        let names = self.names()?;
        let index = *names.get(root).ok_or_else(|| {
            MeaningError::InvalidConstructor(format!("missing graph root `{root}`"))
        })?;
        let mut context = Context {
            graph: self,
            names: &names,
            seen: BTreeMap::new(),
            locals: Vec::new(),
            type_params: Vec::new(),
            quoted: false,
            content: false,
        };
        let mut out = Encoder::default();
        context.global(&mut out, index)?;
        Ok(out.finish())
    }

    pub(crate) fn bytes(&self) -> Result<Vec<u8>, MeaningError> {
        self.encode(false)
    }

    pub(crate) fn content_bytes(&self) -> Result<Vec<u8>, MeaningError> {
        self.encode(true)
    }

    fn encode(&self, content: bool) -> Result<Vec<u8>, MeaningError> {
        let names = self.names()?;
        let mut roots = Vec::new();
        for index in 0..self.declarations.len() {
            let mut context = Context {
                graph: self,
                names: &names,
                seen: BTreeMap::new(),
                locals: Vec::new(),
                type_params: Vec::new(),
                quoted: false,
                content,
            };
            let mut out = Encoder::default();
            // Root membership distinguishes local declarations from dependencies.
            out.bool(self.roots.contains(&self.declarations[index].name));
            context.global(&mut out, index)?;
            roots.push(out.finish());
        }
        roots.sort();
        let mut out = Encoder::default();
        out.text(if content {
            "emath.constructor.content.v1"
        } else {
            "emath.constructor.meaning.v1"
        });
        if content {
            self.texts_for_content(&mut out);
        }
        out.usize(roots.len());
        for root in roots {
            out.blob(&root);
        }
        Ok(out.finish())
    }

    fn texts_for_content(&self, out: &mut Encoder) {
        out.usize(self.aliases.len());
        for (alias, key) in &self.aliases {
            out.text(alias);
            out.text(key);
        }
        out.usize(self.roots.len());
        for root in &self.roots {
            out.text(root);
        }
    }
}

struct Context<'a> {
    graph: &'a ConstructorSemantics,
    names: &'a BTreeMap<String, usize>,
    seen: BTreeMap<usize, usize>,
    locals: Vec<Local>,
    type_params: Vec<String>,
    quoted: bool,
    content: bool,
}

struct Local {
    name: String,
    /// A closure with a scalar domain; nominal projection names stay literal.
    scalar_callable: bool,
}

fn scalar_domain(domain: &Expr) -> bool {
    matches!(&domain.kind, ExprKind::Path { segments, .. }
        if matches!(segments.as_slice(), [name] if matches!(name.as_str(), "Int" | "Rat" | "Float64" | "Bool" | "Unit")))
}

fn scalar_callable_type(ty: &TypeExpr) -> bool {
    matches!(&ty.kind, TypeKind::Fn { domain, .. }
        if matches!(&domain.kind, TypeKind::Path { segments, .. }
            if matches!(segments.as_slice(), [name] if matches!(name.as_str(), "Int" | "Rat" | "Float64" | "Bool" | "Unit"))))
}

fn literal_call_head(segments: &[String]) -> bool {
    matches!(
        segments.last().map(String::as_str),
        Some(
            "int_quot"
                | "int_rem"
                | "int_pow"
                | "int_sum"
                | "int_prod"
                | "buffer"
                | "buffer_set"
                | "numer"
                | "denom"
                | "opaque"
        )
    ) || matches!(segments, [name]
            if matches!(name.as_str(), "constructor_refuse" | "transformation_rule_unavailable"))
        || (segments.len() > 1 && segments[0] == "quote")
        || matches!(segments, [_, method] if method == "pack")
}

impl Context<'_> {
    fn global(&mut self, out: &mut Encoder, index: usize) -> Result<(), MeaningError> {
        if let Some(slot) = self.seen.get(&index) {
            out.tag(0);
            out.usize(*slot);
            return Ok(());
        }
        out.tag(1);
        let slot = self.seen.len();
        self.seen.insert(index, slot);
        out.usize(slot);
        let decl = &self.graph.declarations[index];
        out.bool(self.graph.exports.contains(&decl.name));
        let locals = std::mem::take(&mut self.locals);
        let type_params = std::mem::take(&mut self.type_params);
        let quoted = std::mem::replace(&mut self.quoted, false);
        self.declaration(out, decl)?;
        self.locals = locals;
        self.type_params = type_params;
        self.quoted = quoted;
        Ok(())
    }

    fn path(
        &mut self,
        out: &mut Encoder,
        segments: &[String],
        resolve: bool,
    ) -> Result<(), MeaningError> {
        self.path_in(out, segments, resolve, true)
    }

    fn path_in(
        &mut self,
        out: &mut Encoder,
        segments: &[String],
        resolve: bool,
        local: bool,
    ) -> Result<(), MeaningError> {
        if self.content {
            self.texts(out, segments);
        }
        if local && let Some(first) = segments.first() {
            if let Some(slot) = self.locals.iter().rposition(|local| &local.name == first) {
                out.tag(0);
                out.usize(self.locals.len() - 1 - slot);
                self.texts(out, &segments[1..]);
                return Ok(());
            }
        }
        if resolve {
            // Longest global prefix; suffixes remain nominal field labels.
            for end in (1..=segments.len()).rev() {
                let name = segments[..end].join(".");
                let key = self.graph.aliases.get(&name).unwrap_or(&name);
                if let Some(index) = self.names.get(key) {
                    out.tag(1);
                    self.global(out, *index)?;
                    self.texts(out, &segments[end..]);
                    return Ok(());
                }
            }
        }
        out.tag(2);
        self.texts(out, segments);
        Ok(())
    }

    fn texts(&self, out: &mut Encoder, values: &[String]) {
        out.usize(values.len());
        for value in values {
            out.text(value);
        }
    }

    fn bind(&mut self, name: &str) {
        self.bind_callable(name, false);
    }

    fn bind_callable(&mut self, name: &str, scalar_callable: bool) {
        self.locals.push(Local {
            name: name.to_string(),
            scalar_callable,
        });
    }

    fn scalar_callable(&self, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::FunctionAbs { domain, .. } => scalar_domain(domain),
            ExprKind::Recur { ty, .. } => matches!(&ty.kind,
                ExprKind::Binary { op: BinaryOp::Imply, left, .. } if scalar_domain(left)),
            ExprKind::Path { segments, .. } if segments.len() == 1 => self
                .locals
                .iter()
                .rev()
                .find(|local| local.name == segments[0])
                .is_some_and(|local| local.scalar_callable),
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => self.scalar_callable(then_value) && self.scalar_callable(else_value),
            _ => false,
        }
    }

    fn call_head(&mut self, out: &mut Encoder, function: &Expr) -> Result<(), MeaningError> {
        if let ExprKind::Path { segments, generics } = &function.kind {
            out.tag(7);
            let quoted_free = self.quoted
                && segments
                    .first()
                    .is_some_and(|name| !self.locals.iter().any(|local| &local.name == name));
            let literal = literal_call_head(segments) || quoted_free;
            out.bool(literal);
            if literal {
                self.texts(out, segments);
            }
            self.path(out, segments, true)?;
            out.bool(generics.is_some());
            if let Some(args) = generics {
                self.generic_args(out, args)?;
            }
            // Only a known scalar-domain closure unambiguously shadows a
            // named function. Other calls can use a named/global or nominal
            // field fallback, so retain that edge as well as the value slot.
            let local_callable = segments.len() == 1 && self.scalar_callable(function);
            out.bool(!local_callable);
            if !local_callable {
                self.path_in(out, segments, true, false)?;
            }
        } else {
            self.expr(out, function)?;
        }
        Ok(())
    }

    fn declaration(&mut self, out: &mut Encoder, decl: &Declaration) -> Result<(), MeaningError> {
        out.text(&decl.item_kind);
        out.text(&decl.as_kind);
        let nominal = decl.as_kind == "object";
        let outputs = decl
            .sections()
            .filter(|section| section.name == "outputs")
            .flat_map(|section| section.suite.statements.iter())
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::FieldDecl { name, .. } => Some(name),
                _ => None,
            })
            .collect::<Vec<_>>();
        // A multi-output result exposes this name as its record tag.
        if nominal || self.content || outputs.len() > 1 {
            out.text(&decl.name);
        }
        out.usize(decl.generics.len());
        for generic in &decl.generics {
            if self.content {
                out.text(&generic.name);
            }
            self.bind(&generic.name);
            self.type_params.push(generic.name.clone());
        }
        for generic in &decl.generics {
            self.opt_type(out, generic.bound.as_ref())?;
        }
        // Inputs exist before execution; outputs are bound by definitions.
        for section in decl.sections().filter(|s| s.name == "inputs") {
            for stmt in &section.suite.statements {
                if let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                    self.bind_callable(name, scalar_callable_type(ty));
                }
            }
        }
        out.bool(decl.signature.is_some());
        if let Some(signature) = &decl.signature {
            self.params(out, &signature.params)?;
            self.opt_type(out, signature.ret.as_ref())?;
        }
        // Attributes that admission retains may affect numeric/authority policy.
        out.usize(decl.attributes.len());
        for attribute in &decl.attributes {
            out.text(&attribute.name);
            self.texts(out, &attribute.args);
        }
        self.statements(out, &decl.body, nominal)?;
        out.usize(outputs.len());
        let record = outputs.len() > 1;
        for name in outputs {
            if record {
                out.text(name);
            }
            self.path(out, &[name.clone()], false)?;
        }
        if decl.as_kind == "query" {
            // These are runtime-selected environment roles, not assignments
            // executed from the question/answer metadata sections.
            for name in ["target", "question", "payload", "result"] {
                self.path(out, &[name.to_string()], false)?;
            }
            // An unchecked candidate falls back to the lexically first binding.
            let first = self.locals.iter().map(|local| &local.name).min().cloned();
            out.bool(first.is_some());
            if let Some(first) = first {
                self.path(out, &[first], false)?;
            }
        }
        Ok(())
    }

    fn opt_type(&mut self, out: &mut Encoder, ty: Option<&TypeExpr>) -> Result<(), MeaningError> {
        out.bool(ty.is_some());
        if let Some(ty) = ty {
            self.ty(out, ty)?;
        }
        Ok(())
    }

    fn ty(&mut self, out: &mut Encoder, ty: &TypeExpr) -> Result<(), MeaningError> {
        match &ty.kind {
            TypeKind::Path {
                segments,
                generic_args,
            } => {
                out.tag(0);
                if segments.len() == 1
                    && let Some(slot) = self
                        .type_params
                        .iter()
                        .rposition(|name| name == &segments[0])
                {
                    if self.content {
                        self.texts(out, segments);
                    }
                    out.tag(0);
                    out.usize(self.type_params.len() - 1 - slot);
                    out.usize(0);
                } else {
                    self.path_in(out, segments, false, false)?;
                }
                self.generic_args(out, generic_args)?;
            }
            TypeKind::List(items) | TypeKind::Tuple(items) => {
                out.tag(if matches!(&ty.kind, TypeKind::List(_)) {
                    1
                } else {
                    2
                });
                out.usize(items.len());
                for item in items {
                    self.ty(out, item)?;
                }
            }
            TypeKind::Ref(inner) => {
                out.tag(3);
                self.ty(out, inner)?;
            }
            TypeKind::Product { left, op, right } => {
                out.tag(4);
                out.tag(match op {
                    TypeProductOp::Mul => 0,
                    TypeProductOp::Div => 1,
                });
                self.ty(out, left)?;
                self.ty(out, right)?;
            }
            TypeKind::Pow { base, exponent } => {
                out.tag(5);
                self.ty(out, base)?;
                out.u64(*exponent as i64 as u64);
            }
            TypeKind::In { base, unit } => {
                out.tag(6);
                self.ty(out, base)?;
                self.ty(out, unit)?;
            }
            TypeKind::Domain { base, lo, hi } => {
                out.tag(7);
                self.ty(out, base)?;
                self.expr(out, lo)?;
                self.expr(out, hi)?;
            }
            TypeKind::Fn { domain, codomain } => {
                out.tag(8);
                self.ty(out, domain)?;
                self.ty(out, codomain)?;
            }
        }
        Ok(())
    }

    fn generic_args(&mut self, out: &mut Encoder, args: &[GenericArg]) -> Result<(), MeaningError> {
        out.usize(args.len());
        for arg in args {
            self.generic_arg(out, arg)?;
        }
        Ok(())
    }

    fn generic_arg(&mut self, out: &mut Encoder, arg: &GenericArg) -> Result<(), MeaningError> {
        match arg {
            GenericArg::Type(ty) => {
                out.tag(0);
                self.ty(out, ty)?;
            }
            GenericArg::Value(expr) => {
                out.tag(1);
                self.expr(out, expr)?;
            }
            GenericArg::Named { name, arg } => {
                out.tag(2);
                out.text(name);
                self.generic_arg(out, arg)?;
            }
        }
        Ok(())
    }

    fn opt_expr(&mut self, out: &mut Encoder, value: Option<&Expr>) -> Result<(), MeaningError> {
        out.bool(value.is_some());
        if let Some(value) = value {
            self.expr(out, value)?;
        }
        Ok(())
    }

    fn exprs(&mut self, out: &mut Encoder, values: &[Expr]) -> Result<(), MeaningError> {
        out.usize(values.len());
        for value in values {
            self.expr(out, value)?;
        }
        Ok(())
    }

    fn unit(&mut self, out: &mut Encoder, unit: &UnitExpr) -> Result<(), MeaningError> {
        match unit {
            UnitExpr::Base(name) => {
                out.tag(0);
                self.path(out, &[name.clone()], true)?;
            }
            UnitExpr::Mul(a, b) => {
                out.tag(1);
                self.unit(out, a)?;
                self.unit(out, b)?;
            }
            UnitExpr::Div(a, b) => {
                out.tag(2);
                self.unit(out, a)?;
                self.unit(out, b)?;
            }
            UnitExpr::Pow(base, power) => {
                out.tag(3);
                self.unit(out, base)?;
                out.u64(*power as i64 as u64);
            }
        }
        Ok(())
    }

    fn binder_expr(
        &mut self,
        out: &mut Encoder,
        name: &str,
        domain: &Expr,
        body: &Expr,
    ) -> Result<(), MeaningError> {
        if self.content {
            out.text(name);
        }
        self.domain(out, domain)?;
        let callable = matches!(&domain.kind,
            ExprKind::Binary { op: BinaryOp::Imply, left, .. } if scalar_domain(left));
        self.bind_callable(name, callable);
        self.expr(out, body)?;
        self.locals.pop();
        Ok(())
    }

    fn domain(&mut self, out: &mut Encoder, domain: &Expr) -> Result<(), MeaningError> {
        // Binder domains are type syntax carried as expressions, not calls
        // to same-named functions (including sequence element domains).
        match &domain.kind {
            ExprKind::Path { segments, generics } => {
                out.tag(7);
                if segments.len() == 1
                    && let Some(slot) = self
                        .type_params
                        .iter()
                        .rposition(|name| name == &segments[0])
                {
                    if self.content {
                        self.texts(out, segments);
                    }
                    out.tag(0);
                    out.usize(self.type_params.len() - 1 - slot);
                    out.usize(0);
                } else {
                    self.path_in(out, segments, false, false)?;
                }
                out.bool(generics.is_some());
                if let Some(args) = generics {
                    self.generic_args(out, args)?;
                }
            }
            ExprKind::Call { function, args } => {
                out.tag(8);
                self.domain(out, function)?;
                out.usize(args.len());
                for arg in args {
                    self.domain(out, arg)?;
                }
            }
            ExprKind::Binary {
                op: BinaryOp::Imply,
                left,
                right,
            } => {
                out.tag(12);
                out.tag(13);
                self.domain(out, left)?;
                self.domain(out, right)?;
            }
            _ => self.expr(out, domain)?,
        }
        Ok(())
    }

    fn policy_value(&mut self, out: &mut Encoder, value: &Expr) -> Result<(), MeaningError> {
        if let ExprKind::Path { segments, generics } = &value.kind {
            out.tag(7);
            self.path_in(out, segments, false, false)?;
            out.bool(generics.is_some());
            if let Some(args) = generics {
                self.generic_args(out, args)?;
            }
            Ok(())
        } else {
            self.expr(out, value)
        }
    }

    fn expr(&mut self, out: &mut Encoder, value: &Expr) -> Result<(), MeaningError> {
        match &value.kind {
            ExprKind::Int(text) => {
                out.tag(0);
                out.text(&text.replace('_', ""));
            }
            ExprKind::Float(text) => {
                out.tag(1);
                out.text(&text.replace('_', ""));
            }
            ExprKind::Rational { numer, denom } => {
                out.tag(2);
                out.text(&numer.replace('_', ""));
                out.text(&denom.replace('_', ""));
            }
            ExprKind::Str(text) => {
                out.tag(3);
                out.text(text);
            }
            ExprKind::Bool(value) => {
                out.tag(4);
                out.bool(*value);
            }
            ExprKind::Quantity { value, unit } => {
                out.tag(5);
                self.expr(out, value)?;
                self.unit(out, unit)?;
            }
            ExprKind::Measured {
                value,
                uncertainty,
                uncertainty_digits,
                distribution,
            } => {
                out.tag(6);
                out.text(value);
                out.text(uncertainty);
                out.text(uncertainty_digits);
                out.bool(distribution.is_some());
                if let Some(distribution) = distribution {
                    out.text(distribution);
                }
            }
            ExprKind::Path { segments, generics } => {
                out.tag(7);
                self.path(out, segments, !self.quoted)?;
                out.bool(generics.is_some());
                if let Some(args) = generics {
                    self.generic_args(out, args)?;
                }
            }
            ExprKind::Call { function, args } => {
                out.tag(8);
                self.call_head(out, function)?;
                let substitute = matches!(&function.kind, ExprKind::Path { segments, .. }
                    if segments.as_slice() == ["quote", "substitute"]);
                out.usize(args.len());
                for (index, arg) in args.iter().enumerate() {
                    if substitute
                        && index == 1
                        && matches!(&arg.kind, ExprKind::Path { segments, .. } if segments.len() == 1)
                    {
                        self.policy_value(out, arg)?;
                    } else {
                        self.expr(out, arg)?;
                    }
                }
            }
            ExprKind::Index { value, indices } => {
                out.tag(9);
                self.expr(out, value)?;
                self.exprs(out, indices)?;
            }
            ExprKind::Slice { start, end } => {
                out.tag(10);
                self.opt_expr(out, start.as_deref())?;
                self.opt_expr(out, end.as_deref())?;
            }
            ExprKind::Unary { op, value } => {
                out.tag(11);
                out.tag(match op {
                    UnaryOp::Neg => 0,
                    UnaryOp::Pos => 1,
                    UnaryOp::Not => 2,
                });
                self.expr(out, value)?;
            }
            ExprKind::Binary { op, left, right } => {
                out.tag(12);
                out.tag(match op {
                    BinaryOp::Add => 0,
                    BinaryOp::Sub => 1,
                    BinaryOp::Mul => 2,
                    BinaryOp::Div => 3,
                    BinaryOp::Pow => 4,
                    BinaryOp::Eq => 5,
                    BinaryOp::Ne => 6,
                    BinaryOp::Lt => 7,
                    BinaryOp::Le => 8,
                    BinaryOp::Gt => 9,
                    BinaryOp::Ge => 10,
                    BinaryOp::And => 11,
                    BinaryOp::Or => 12,
                    BinaryOp::Imply => 13,
                    BinaryOp::Iff => 14,
                    BinaryOp::Asymp => 15,
                    BinaryOp::In => 16,
                });
                self.expr(out, left)?;
                self.expr(out, right)?;
            }
            ExprKind::Approx {
                left,
                right,
                tolerance,
            } => {
                out.tag(13);
                self.expr(out, left)?;
                self.expr(out, right)?;
                out.bool(tolerance.is_some());
                if let Some(tolerance) = tolerance {
                    self.opt_expr(out, tolerance.rtol.as_ref())?;
                    self.opt_expr(out, tolerance.atol.as_ref())?;
                }
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                out.tag(14);
                self.expr(out, condition)?;
                self.expr(out, then_value)?;
                self.expr(out, else_value)?;
            }
            ExprKind::List(items) => {
                out.tag(15);
                self.exprs(out, items)?;
            }
            ExprKind::Table { headers, rows } => {
                out.tag(16);
                self.texts(out, headers);
                out.usize(rows.len());
                for row in rows {
                    self.exprs(out, row)?;
                }
            }
            ExprKind::Set(items) => {
                out.tag(17);
                self.exprs(out, items)?;
            }
            ExprKind::SetComprehension {
                element,
                var,
                domain,
                guard,
            } => {
                if self.content {
                    out.text(var);
                }
                out.tag(18);
                self.expr(out, domain)?;
                self.bind(var);
                self.expr(out, element)?;
                self.opt_expr(out, guard.as_deref())?;
                self.locals.pop();
            }
            ExprKind::Record { type_path, fields } => {
                out.tag(19);
                self.path_in(out, type_path, false, false)?;
                out.usize(fields.len());
                for (name, value) in fields {
                    out.text(name);
                    self.expr(out, value)?;
                }
            }
            ExprKind::Tuple(items) => {
                out.tag(20);
                self.exprs(out, items)?;
            }
            ExprKind::WithSeriesPolicy {
                value,
                interpolation,
                extrapolation,
            } => {
                out.tag(21);
                self.expr(out, value)?;
                out.bool(interpolation.is_some());
                if let Some(policy) = interpolation {
                    out.text(policy.spelling());
                }
                out.bool(extrapolation.is_some());
                if let Some(policy) = extrapolation {
                    out.text(policy.spelling());
                }
            }
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                out.tag(22);
                self.opt_expr(out, start.as_deref())?;
                self.opt_expr(out, end.as_deref())?;
                out.bool(*inclusive);
            }
            ExprKind::Conditioned { value, condition } => {
                out.tag(23);
                self.expr(out, value)?;
                self.expr(out, condition)?;
            }
            ExprKind::UnitQuery { kind, expr } => {
                out.tag(24);
                out.tag(match kind {
                    UnitQueryKind::Unit => 0,
                    UnitQueryKind::Dimension => 1,
                });
                self.expr(out, expr)?;
            }
            ExprKind::Cases {
                subject,
                arms,
                else_arm,
            } => {
                out.tag(25);
                self.opt_expr(out, subject.as_deref())?;
                out.usize(arms.len());
                for (condition, value) in arms {
                    self.expr(out, condition)?;
                    self.expr(out, value)?;
                }
                self.expr(out, else_arm)?;
            }
            ExprKind::FunctionAbs {
                param,
                domain,
                body,
            } => {
                out.tag(26);
                self.binder_expr(out, param, domain, body)?;
            }
            ExprKind::Recur { name, ty, body } => {
                out.tag(27);
                self.binder_expr(out, name, ty, body)?;
            }
            ExprKind::Quote { body } => {
                out.tag(28);
                let old = std::mem::replace(&mut self.quoted, true);
                let locals = std::mem::take(&mut self.locals);
                self.expr(out, body)?;
                self.locals = locals;
                self.quoted = old;
            }
            ExprKind::QuoteBind {
                param,
                domain,
                body,
            } => {
                out.tag(29);
                let old = std::mem::replace(&mut self.quoted, true);
                let locals = if old {
                    None
                } else {
                    Some(std::mem::take(&mut self.locals))
                };
                self.binder_expr(out, param, domain, body)?;
                if let Some(locals) = locals {
                    self.locals = locals;
                }
                self.quoted = old;
            }
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => {
                out.tag(30);
                self.expr(out, callee)?;
                let literal = matches!(&callee.kind, ExprKind::Path { segments, .. }
                    if segments.first().is_some_and(|name| name == "quote")
                        || segments.last().is_some_and(|name| name == "open"));
                out.bool(literal);
                if literal && let ExprKind::Path { segments, .. } = &callee.kind {
                    self.texts(out, segments);
                }
                if matches!(&callee.kind, ExprKind::Path { segments, .. }
                    if segments.as_slice() == ["quote", "bind"])
                {
                    let old = std::mem::replace(&mut self.quoted, true);
                    let locals = if old {
                        None
                    } else {
                        Some(std::mem::take(&mut self.locals))
                    };
                    self.binder_expr(out, param, domain, body)?;
                    if let Some(locals) = locals {
                        self.locals = locals;
                    }
                    self.quoted = old;
                } else {
                    if self.content {
                        out.text(param);
                    }
                    self.expr(out, domain)?;
                    self.bind(param);
                    self.expr(out, body)?;
                    self.locals.pop();
                }
            }
            ExprKind::SequenceCons {
                head,
                tail,
                spread_head,
            } => {
                out.tag(31);
                out.bool(*spread_head);
                self.expr(out, head)?;
                self.expr(out, tail)?;
            }
        }
        Ok(())
    }

    fn visibility(&self, out: &mut Encoder, visibility: Option<Visibility>) {
        out.tag(match visibility {
            None => 0,
            Some(Visibility::Public) => 1,
            Some(Visibility::Package) => 2,
            Some(Visibility::Private) => 3,
        });
    }

    fn params(&mut self, out: &mut Encoder, params: &[Param]) -> Result<(), MeaningError> {
        out.usize(params.len());
        for param in params {
            if self.content {
                out.text(&param.name);
            }
            self.ty(out, &param.ty)?;
            out.bool(param.by_ref);
            self.opt_expr(out, param.default.as_ref())?;
            self.bind_callable(&param.name, scalar_callable_type(&param.ty));
        }
        Ok(())
    }

    fn statements(
        &mut self,
        out: &mut Encoder,
        statements: &[Stmt],
        nominal: bool,
    ) -> Result<(), MeaningError> {
        let retained = statements.iter().filter(|stmt| {
            self.content || !matches!(&stmt.kind, StmtKind::Section(section) if matches!(section.name.as_str(), "tests" | "about" | "evidence" | "host"))
        }).collect::<Vec<_>>();
        out.usize(retained.len());
        for statement in retained {
            self.statement(out, statement, nominal)?;
        }
        Ok(())
    }

    fn suite(
        &mut self,
        out: &mut Encoder,
        suite: &Suite,
        nominal: bool,
    ) -> Result<(), MeaningError> {
        let depth = self.locals.len();
        self.statements(out, &suite.statements, nominal)?;
        self.locals.truncate(depth);
        Ok(())
    }

    fn statement(
        &mut self,
        out: &mut Encoder,
        stmt: &Stmt,
        nominal: bool,
    ) -> Result<(), MeaningError> {
        match &stmt.kind {
            StmtKind::Section(section) => {
                out.tag(0);
                out.text(&section.name);
                out.bool(section.generic.is_some());
                if let Some(generic) = &section.generic {
                    self.path(out, &[generic.clone()], true)?;
                }
                out.bool(section.args.is_some());
                if let Some(args) = &section.args {
                    out.usize(args.len());
                    for arg in args {
                        out.bool(arg.name.is_some());
                        if let Some(name) = &arg.name {
                            out.text(name);
                        }
                        match &arg.value {
                            ArgumentValue::Expr(value) => {
                                out.tag(0);
                                self.expr(out, value)?;
                            }
                            ArgumentValue::Type(ty) => {
                                out.tag(1);
                                self.ty(out, ty)?;
                            }
                        }
                    }
                }
                // Output declarations describe slots but do not bind values
                // before the executable definitions that produce them.
                let depth = self.locals.len();
                let policy = matches!(
                    section.name.as_str(),
                    "representation" | "exports" | "using" | "answer"
                );
                if policy {
                    out.usize(section.suite.statements.len());
                    for statement in &section.suite.statements {
                        match &statement.kind {
                            StmtKind::Assign { target, value } => {
                                out.tag(5);
                                self.policy_value(out, value)?;
                                self.texts(out, &target.segments);
                                self.exprs(out, &target.indices)?;
                            }
                            StmtKind::FieldDecl {
                                visibility,
                                name,
                                ty,
                                default,
                            } => {
                                out.tag(1);
                                self.visibility(out, *visibility);
                                self.ty(out, ty)?;
                                out.bool(default.is_some());
                                if let Some(value) = default {
                                    self.policy_value(out, value)?;
                                }
                                out.text(name);
                            }
                            _ => self.statement(out, statement, true)?,
                        }
                    }
                } else {
                    self.statements(
                        out,
                        &section.suite.statements,
                        nominal
                            || matches!(
                                section.name.as_str(),
                                "representation"
                                    | "exports"
                                    | "question"
                                    | "using"
                                    | "answer"
                                    | "budget"
                                    | "invariants"
                            ),
                    )?;
                }
                if section.name == "outputs" {
                    self.locals.truncate(depth);
                }
            }
            StmtKind::FieldDecl {
                visibility,
                name,
                ty,
                default,
            } => {
                out.tag(1);
                self.visibility(out, *visibility);
                self.ty(out, ty)?;
                self.opt_expr(out, default.as_ref())?;
                if nominal {
                    out.text(name);
                } else {
                    if !self.locals.iter().any(|local| &local.name == name) {
                        self.bind_callable(name, scalar_callable_type(ty));
                    }
                    self.path(out, &[name.clone()], false)?;
                }
            }
            StmtKind::FnDecl {
                visibility,
                head,
                name,
                params,
                ret,
                suite,
                ..
            } => {
                out.tag(2);
                self.visibility(out, *visibility);
                out.text(head);
                if nominal {
                    out.text(name);
                } else {
                    self.bind(name);
                    self.path(out, &[name.clone()], false)?;
                }
                let depth = self.locals.len();
                self.params(out, params)?;
                self.opt_type(out, ret.as_ref())?;
                out.bool(suite.is_some());
                if let Some(suite) = suite {
                    self.suite(out, suite, false)?;
                }
                self.locals.truncate(depth);
            }
            StmtKind::OperatorDecl {
                name, params, ret, ..
            } => {
                out.tag(3);
                out.text(name);
                let depth = self.locals.len();
                self.params(out, params)?;
                self.opt_type(out, ret.as_ref())?;
                self.locals.truncate(depth);
            }
            StmtKind::Let { name, ty, value } => {
                if self.content {
                    out.text(name);
                }
                out.tag(4);
                self.opt_type(out, ty.as_ref())?;
                self.expr(out, value)?;
                let callable = self.scalar_callable(value);
                self.bind_callable(name, callable);
            }
            StmtKind::Assign { target, value } => {
                out.tag(5);
                self.expr(out, value)?;
                if nominal {
                    self.texts(out, &target.segments);
                } else {
                    if let [name] = target.segments.as_slice() {
                        let callable = self.scalar_callable(value);
                        if let Some(local) = self
                            .locals
                            .iter_mut()
                            .rev()
                            .find(|local| &local.name == name)
                        {
                            local.scalar_callable = callable;
                        } else {
                            self.bind_callable(name, callable);
                        }
                    }
                    self.path(out, &target.segments, true)?;
                }
                self.exprs(out, &target.indices)?;
            }
            StmtKind::Require(value) => {
                out.tag(6);
                self.expr(out, value)?;
            }
            StmtKind::Ensure(value) => {
                out.tag(7);
                self.expr(out, value)?;
            }
            StmtKind::Invariant(value) => {
                out.tag(8);
                self.expr(out, value)?;
            }
            StmtKind::Given { name, value } => {
                out.tag(9);
                if self.content {
                    out.text(name);
                }
                self.expr(out, value)?;
                self.bind(name);
            }
            StmtKind::Expect(value) => {
                out.tag(10);
                self.expr(out, value)?;
            }
            StmtKind::Expr(value) => {
                out.tag(11);
                self.expr(out, value)?;
            }
            StmtKind::If {
                condition,
                then,
                else_branches,
                else_tail,
            } => {
                out.tag(12);
                self.expr(out, condition)?;
                self.suite(out, then, nominal)?;
                out.usize(else_branches.len());
                for (condition, suite) in else_branches {
                    self.expr(out, condition)?;
                    self.suite(out, suite, nominal)?;
                }
                out.bool(else_tail.is_some());
                if let Some(suite) = else_tail {
                    self.suite(out, suite, nominal)?;
                }
            }
            StmtKind::BinderStmt {
                kind,
                binders,
                suite,
                guard,
            } => {
                out.tag(13);
                out.tag(match kind {
                    BinderKind::Sum => 0,
                    BinderKind::Product => 1,
                    BinderKind::Integral => 2,
                    BinderKind::ForAll => 3,
                    BinderKind::Exists => 4,
                    BinderKind::Series => 5,
                });
                let depth = self.locals.len();
                out.usize(binders.len());
                for binder in binders {
                    if self.content {
                        out.text(&binder.name);
                    }
                    self.opt_expr(out, binder.domain.as_ref())?;
                    self.bind(&binder.name);
                }
                self.opt_expr(out, guard.as_deref())?;
                self.suite(out, suite, nominal)?;
                self.locals.truncate(depth);
            }
            StmtKind::SelfBlock { assignments } => {
                out.tag(14);
                out.usize(assignments.len());
                for (field, value) in assignments {
                    out.text(field);
                    self.expr(out, value)?;
                }
            }
            StmtKind::Command { head, argument } => {
                out.tag(15);
                self.texts(out, head);
                out.bool(argument.is_some());
                if let Some(argument) = argument {
                    match argument {
                        CommandArgument::Expr(value) => {
                            out.tag(0);
                            self.expr(out, value)?;
                        }
                        CommandArgument::Assignment { name, value } => {
                            out.tag(1);
                            out.text(name);
                            self.expr(out, value)?;
                        }
                        CommandArgument::List(values) => {
                            out.tag(2);
                            self.exprs(out, values)?;
                        }
                    }
                }
            }
            StmtKind::Equation { left, right } => {
                out.tag(16);
                self.expr(out, left)?;
                self.expr(out, right)?;
            }
            StmtKind::Reaction {
                name,
                lhs,
                arrow,
                rhs,
            } => {
                out.tag(17);
                out.text(name);
                out.text(arrow.as_str());
                for terms in [lhs, rhs] {
                    out.usize(terms.len());
                    for term in terms {
                        out.u64(term.coefficient);
                        out.text(&term.species);
                    }
                }
            }
        }
        Ok(())
    }
}

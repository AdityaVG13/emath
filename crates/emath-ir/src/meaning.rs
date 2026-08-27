//! Canonical admitted meaning identity.
//!
//! Meaning identity is deliberately narrower than source/content identity:
//! presentation, declaration/local/binder names, tests, evidence attachments,
//! and host bindings do not enter the preimage. Admitted types, expressions,
//! goals, numeric policy, unresolved-meaning state, and dependency meanings do.

use crate::constructor::{Field, Visibility};
use crate::expression::{BinderKind, ExprNode, Literal, SliceAxis};
use crate::goal::{DeterminismPolicy, ExactnessPolicy, FallbackPolicy, GoalKind};
use crate::ids::{ExprId, GoalId, TypeId};
use crate::package::{Declaration, ImportSelection, SemanticPackage};
use crate::types::TypeNode;
use emath_core::MeaningId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Version of the canonical admitted-meaning rules.
pub const MEANING_CANONICAL_SCHEMA_V1: &str = "emath.meaning.canonical.v1";

/// Malformed or internally inconsistent SIR cannot be assigned a `MeaningID`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeaningError {
    MissingExpr(ExprId),
    MissingGoal(GoalId),
    MissingType(TypeId),
    CyclicExpr(ExprId),
    CyclicDefinition(String),
}

impl fmt::Display for MeaningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingExpr(id) => write!(formatter, "missing SIR expression {}", id.0),
            Self::MissingGoal(id) => write!(formatter, "missing SIR goal {}", id.0),
            Self::MissingType(id) => write!(formatter, "missing SIR type {}", id.0),
            Self::CyclicExpr(id) => write!(formatter, "cyclic SIR expression {}", id.0),
            Self::CyclicDefinition(name) => {
                write!(formatter, "cyclic admitted definition `{name}`")
            }
        }
    }
}

impl std::error::Error for MeaningError {}

#[derive(Default)]
struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn tag(&mut self, tag: u8) {
        self.bytes.push(tag);
    }

    fn bool(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn usize(&mut self, value: usize) {
        self.u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    fn text(&mut self, value: &str) {
        self.usize(value.len());
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn blob(&mut self, value: &[u8]) {
        self.usize(value.len());
        self.bytes.extend_from_slice(value);
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Clone)]
struct LocalSlot {
    role: u8,
    index: usize,
}

struct MeaningContext<'a> {
    package: &'a SemanticPackage,
    locals: BTreeMap<String, LocalSlot>,
    definitions: &'a BTreeMap<String, ExprId>,
    aliases: BTreeMap<String, String>,
    bound: Vec<String>,
    active_exprs: BTreeSet<ExprId>,
    active_definitions: BTreeSet<String>,
}

impl MeaningContext<'_> {
    fn encode_name(&mut self, out: &mut Encoder, name: &str) -> Result<(), MeaningError> {
        if let Some(position) = self.bound.iter().rposition(|bound| bound == name) {
            out.tag(0);
            out.usize(self.bound.len() - position - 1);
            return Ok(());
        }
        let local_name = name.strip_prefix("state.").unwrap_or(name);
        if let Some(slot) = self.locals.get(local_name) {
            out.tag(1);
            out.tag(slot.role);
            out.usize(slot.index);
            return Ok(());
        }
        if let Some(expression) = self.definitions.get(name).copied() {
            if !self.active_definitions.insert(name.to_string()) {
                return Err(MeaningError::CyclicDefinition(name.to_string()));
            }
            out.tag(2);
            self.encode_expr(out, expression)?;
            self.active_definitions.remove(name);
            return Ok(());
        }
        out.tag(3);
        out.text(self.aliases.get(name).map_or(name, String::as_str));
        Ok(())
    }

    fn encode_expr(&mut self, out: &mut Encoder, id: ExprId) -> Result<(), MeaningError> {
        if !self.active_exprs.insert(id) {
            return Err(MeaningError::CyclicExpr(id));
        }
        let expr = self
            .package
            .exprs
            .get(id.index())
            .ok_or(MeaningError::MissingExpr(id))?;
        match expr {
            ExprNode::Literal(literal) => {
                out.tag(0);
                match literal {
                    Literal::Bool(value) => {
                        out.tag(0);
                        out.bool(*value);
                    }
                    Literal::Integer(value) => {
                        out.tag(1);
                        out.text(value);
                    }
                    Literal::Rational(value) => {
                        out.tag(2);
                        out.text(value);
                    }
                    Literal::FloatBits(bits) => {
                        out.tag(3);
                        out.u64(*bits);
                    }
                    Literal::Complex { re_bits, im_bits } => {
                        out.tag(4);
                        out.u64(*re_bits);
                        out.u64(*im_bits);
                    }
                    Literal::Text(value) => {
                        out.tag(5);
                        out.text(value);
                    }
                }
            }
            ExprNode::Variable(name) => {
                out.tag(1);
                self.encode_name(out, &name.0)?;
            }
            ExprNode::Call {
                function,
                arguments,
            } => {
                out.tag(2);
                self.encode_name(out, &function.0)?;
                out.usize(arguments.len());
                for argument in arguments {
                    self.encode_expr(out, *argument)?;
                }
            }
            ExprNode::Unary { operation, value } => {
                out.tag(3);
                out.text(operation.name());
                self.encode_expr(out, *value)?;
            }
            ExprNode::Binary {
                operation,
                left,
                right,
            } => {
                out.tag(4);
                out.text(operation.name());
                self.encode_expr(out, *left)?;
                self.encode_expr(out, *right)?;
            }
            ExprNode::If {
                condition,
                then_value,
                else_value,
            } => {
                out.tag(5);
                self.encode_expr(out, *condition)?;
                self.encode_expr(out, *then_value)?;
                self.encode_expr(out, *else_value)?;
            }
            ExprNode::Record { fields, ty } => {
                out.tag(6);
                encode_type_id(out, self.package, *ty)?;
                out.usize(fields.len());
                for (field, value) in fields {
                    out.text(field);
                    self.encode_expr(out, *value)?;
                }
            }
            ExprNode::Index { value, indices } => {
                out.tag(7);
                self.encode_expr(out, *value)?;
                out.usize(indices.len());
                for index in indices {
                    self.encode_expr(out, *index)?;
                }
            }
            ExprNode::Slice { value, axes } => {
                out.tag(8);
                self.encode_expr(out, *value)?;
                out.usize(axes.len());
                for axis in axes {
                    match axis {
                        SliceAxis::Point(index) => {
                            out.tag(0);
                            self.encode_expr(out, *index)?;
                        }
                        SliceAxis::Range { start, end } => {
                            out.tag(1);
                            self.encode_expr(out, *start)?;
                            self.encode_expr(out, *end)?;
                        }
                    }
                }
            }
            ExprNode::Binder {
                kind,
                variables,
                body,
            } => {
                out.tag(9);
                out.tag(match kind {
                    BinderKind::Sum => 0,
                    BinderKind::Product => 1,
                    BinderKind::Integral => 2,
                    BinderKind::ForAll => 3,
                    BinderKind::Exists => 4,
                    BinderKind::Series => 5,
                });
                out.usize(variables.len());
                let bound_len = self.bound.len();
                for variable in variables {
                    self.encode_expr(out, variable.domain)?;
                    self.bound.push(variable.name.clone());
                }
                self.encode_expr(out, *body)?;
                self.bound.truncate(bound_len);
            }
            ExprNode::Vector(elements) => {
                out.tag(10);
                out.usize(elements.len());
                for element in elements {
                    self.encode_expr(out, *element)?;
                }
            }
            ExprNode::Matrix(rows) => {
                out.tag(11);
                out.usize(rows.len());
                for row in rows {
                    out.usize(row.len());
                    for element in row {
                        self.encode_expr(out, *element)?;
                    }
                }
            }
            ExprNode::Tensor { shape, elements } => {
                out.tag(12);
                out.usize(shape.len());
                for extent in shape {
                    out.usize(*extent);
                }
                out.usize(elements.len());
                for element in elements {
                    self.encode_expr(out, *element)?;
                }
            }
            ExprNode::Differentiate { body, var } => {
                out.tag(13);
                self.encode_name(out, var)?;
                self.encode_expr(out, *body)?;
            }
            ExprNode::Solve { body, var } => {
                out.tag(14);
                self.encode_name(out, var)?;
                self.encode_expr(out, *body)?;
            }
            ExprNode::Optimize {
                body,
                vars,
                maximize,
            } => {
                out.tag(15);
                out.bool(*maximize);
                out.usize(vars.len());
                for var in vars {
                    self.encode_name(out, var)?;
                }
                self.encode_expr(out, *body)?;
            }
            ExprNode::SampleLimit {
                body,
                var,
                target,
                direction,
            } => {
                out.tag(16);
                self.encode_name(out, var)?;
                self.encode_expr(out, *target)?;
                self.encode_expr(out, *direction)?;
                self.encode_expr(out, *body)?;
            }
        }
        self.active_exprs.remove(&id);
        Ok(())
    }
}

fn encode_type_id(
    out: &mut Encoder,
    package: &SemanticPackage,
    id: TypeId,
) -> Result<(), MeaningError> {
    let ty = package
        .types
        .get(id.index())
        .ok_or(MeaningError::MissingType(id))?;
    encode_type(out, ty);
    Ok(())
}

fn encode_type(out: &mut Encoder, ty: &TypeNode) {
    match ty {
        TypeNode::Bool => out.tag(0),
        TypeNode::Nat => out.tag(1),
        TypeNode::Int => out.tag(2),
        TypeNode::Rational => out.tag(3),
        TypeNode::Float64 => out.tag(4),
        TypeNode::Refinement { base, predicate } => {
            out.tag(5);
            encode_type(out, base);
            out.text(predicate);
        }
        TypeNode::Interval(inner) => {
            out.tag(6);
            encode_type(out, inner);
        }
        TypeNode::Complex(inner) => {
            out.tag(7);
            encode_type(out, inner);
        }
        TypeNode::Vector { element, extent } => {
            out.tag(8);
            encode_type(out, element);
            out.bool(extent.is_some());
            if let Some(extent) = extent {
                out.text(&extent.to_string());
            }
        }
        TypeNode::Matrix {
            element,
            rows,
            cols,
        } => {
            out.tag(9);
            encode_type(out, element);
            out.bool(rows.is_some());
            if let Some(rows) = rows {
                out.text(&rows.to_string());
            }
            out.bool(cols.is_some());
            if let Some(cols) = cols {
                out.text(&cols.to_string());
            }
        }
        TypeNode::Tensor { element, shape } => {
            out.tag(10);
            encode_type(out, element);
            out.usize(shape.len());
            for extent in shape {
                out.text(&extent.to_string());
            }
        }
        TypeNode::Record(name) => {
            out.tag(11);
            out.text(&name.0);
        }
        TypeNode::Variant(name) => {
            out.tag(12);
            out.text(&name.0);
        }
        TypeNode::Result { ok, error } => {
            out.tag(13);
            encode_type(out, ok);
            encode_type(out, error);
        }
        TypeNode::OptionType(inner) => {
            out.tag(14);
            encode_type(out, inner);
        }
        TypeNode::Opaque {
            name,
            provider_contract,
        } => {
            out.tag(15);
            out.text(&name.0);
            out.bool(provider_contract.is_some());
            if let Some(contract) = provider_contract {
                out.text(&contract.0);
            }
        }
        TypeNode::UnitRef { dims, family, .. } => {
            out.tag(16);
            out.text(family.as_str());
            out.text(&dims.render());
        }
        TypeNode::Other(name) => {
            out.tag(17);
            out.text(&name.0);
        }
    }
}

fn alias_map(package: &SemanticPackage) -> BTreeMap<String, String> {
    let mut aliases = BTreeMap::new();
    for import in &package.imports {
        if let ImportSelection::Named(names) = &import.selection {
            for (name, alias) in names {
                let local = alias.as_ref().unwrap_or(name);
                let mut qualified = import.path.join("::");
                if !qualified.is_empty() {
                    qualified.push_str("::");
                }
                qualified.push_str(name);
                aliases.insert(local.clone(), qualified);
            }
        }
    }
    aliases
}

fn add_fields(
    out: &mut Encoder,
    package: &SemanticPackage,
    locals: &mut BTreeMap<String, LocalSlot>,
    role: u8,
    fields: &[Field],
) -> Result<(), MeaningError> {
    out.tag(role);
    out.usize(fields.len());
    for (index, field) in fields.iter().enumerate() {
        encode_type_id(out, package, field.ty)?;
        out.tag(match field.visibility {
            Visibility::Public => 0,
            Visibility::Package => 1,
            Visibility::Private => 2,
        });
        locals.insert(field.name.clone(), LocalSlot { role, index });
    }
    Ok(())
}

fn encode_exactness(out: &mut Encoder, policy: &ExactnessPolicy) {
    match policy {
        ExactnessPolicy::Exact => out.tag(0),
        ExactnessPolicy::Bounded { tolerance_literal } => {
            out.tag(1);
            out.text(tolerance_literal);
        }
        ExactnessPolicy::CheckedNumeric => out.tag(2),
        ExactnessPolicy::Estimate => out.tag(3),
        ExactnessPolicy::AnyExplicit => out.tag(4),
    }
}

fn encode_declaration(
    package: &SemanticPackage,
    declaration: &Declaration,
    aliases: &BTreeMap<String, String>,
) -> Result<Vec<u8>, MeaningError> {
    let mut out = Encoder::default();
    out.text(
        aliases
            .get(&declaration.kind.0)
            .map_or(&declaration.kind.0, String::as_str),
    );
    let mut locals = BTreeMap::new();
    add_fields(&mut out, package, &mut locals, 0, &declaration.inputs)?;
    add_fields(&mut out, package, &mut locals, 1, &declaration.outputs)?;
    add_fields(&mut out, package, &mut locals, 2, &declaration.state)?;
    add_fields(&mut out, package, &mut locals, 3, &declaration.algebraic)?;

    let mut context = MeaningContext {
        package,
        locals,
        definitions: &declaration.definitions,
        aliases: aliases.clone(),
        bound: Vec::new(),
        active_exprs: BTreeSet::new(),
        active_definitions: BTreeSet::new(),
    };

    let mut definitions = Vec::new();
    for (name, expression) in &declaration.definitions {
        context.active_definitions.insert(name.clone());
        let mut encoded = Encoder::default();
        context.encode_expr(&mut encoded, *expression)?;
        context.active_definitions.remove(name);
        definitions.push(encoded.finish());
    }
    definitions.sort();
    out.tag(4);
    out.usize(definitions.len());
    for definition in definitions {
        out.blob(&definition);
    }

    out.tag(5);
    out.usize(declaration.invariants.len());
    for invariant in &declaration.invariants {
        context.encode_expr(&mut out, *invariant)?;
    }

    out.tag(6);
    out.usize(declaration.constructors.len());
    for constructor in &declaration.constructors {
        let declaration_locals = context.locals.clone();
        out.usize(constructor.parameters.len());
        for (index, parameter) in constructor.parameters.iter().enumerate() {
            encode_type_id(&mut out, package, parameter.ty)?;
            context
                .locals
                .insert(parameter.name.clone(), LocalSlot { role: 4, index });
        }
        out.usize(constructor.preconditions.len());
        for precondition in &constructor.preconditions {
            context.encode_expr(&mut out, *precondition)?;
        }
        out.usize(constructor.assignments.len());
        for (field, value) in &constructor.assignments {
            context.encode_name(&mut out, field)?;
            context.encode_expr(&mut out, *value)?;
        }
        out.usize(constructor.postconditions.len());
        for postcondition in &constructor.postconditions {
            context.encode_expr(&mut out, *postcondition)?;
        }
        out.usize(constructor.defaults.len());
        for (field, value) in &constructor.defaults {
            context.encode_name(&mut out, field)?;
            context.encode_expr(&mut out, *value)?;
        }
        out.bool(constructor.error_type.is_some());
        if let Some(error_type) = constructor.error_type {
            encode_type_id(&mut out, package, error_type)?;
        }
        out.bool(constructor.is_public);
        context.locals = declaration_locals;
    }

    out.tag(7);
    out.text(declaration.compile_spec.numeric.as_str());
    out.bool(declaration.compile_spec.unresolved.is_some());

    out.tag(8);
    out.usize(declaration.goals.len());
    for goal_id in &declaration.goals {
        let goal = package
            .goals
            .get(goal_id.index())
            .ok_or(MeaningError::MissingGoal(*goal_id))?;
        match &goal.kind {
            GoalKind::Custom(schema) => {
                out.tag(11);
                out.text(&schema.0);
            }
            kind => out.tag(match kind {
                GoalKind::Evaluate => 0,
                GoalKind::Differentiate => 1,
                GoalKind::Integrate => 2,
                GoalKind::Solve => 3,
                GoalKind::Optimize => 4,
                GoalKind::Simulate => 5,
                GoalKind::Search => 6,
                GoalKind::Prove => 7,
                GoalKind::Verify => 8,
                GoalKind::Compile => 9,
                GoalKind::Benchmark => 10,
                GoalKind::Transform => 11,
                GoalKind::Simplify => 12,
                GoalKind::Custom(_) => unreachable!(),
            }),
        }
        context.encode_name(&mut out, &goal.target)?;
        out.bool(goal.expression.is_some());
        if let Some(expression) = goal.expression {
            context.encode_expr(&mut out, expression)?;
        }
        encode_exactness(&mut out, &goal.requirements.exactness);
        out.tag(match goal.requirements.determinism {
            DeterminismPolicy::Required => 0,
            DeterminismPolicy::Preferred => 1,
            DeterminismPolicy::Unspecified => 2,
        });
        out.tag(match goal.requirements.fallback {
            FallbackPolicy::NativeOnly => 0,
            FallbackPolicy::Parametric => 1,
            FallbackPolicy::Continuation => 2,
            FallbackPolicy::Diagnostic => 3,
            FallbackPolicy::ExplicitLadder => 4,
        });
        out.text(&goal.requirements.target.family);
        out.bool(goal.requirements.target.triple.is_some());
        if let Some(triple) = &goal.requirements.target.triple {
            out.text(triple);
        }
        let mut features = goal.requirements.target.features.clone();
        features.sort();
        features.dedup();
        out.usize(features.len());
        for feature in features {
            out.text(&feature);
        }
        out.text(&goal.requirements.produce);
        out.usize(goal.payload.wrt.len());
        for name in &goal.payload.wrt {
            context.encode_name(&mut out, name)?;
        }
        out.bool(goal.payload.order.is_some());
        if let Some(order) = goal.payload.order {
            out.u64(u64::from(order));
        }
        out.bool(goal.payload.against.is_some());
        if let Some(against) = &goal.payload.against {
            out.text(against);
        }
        out.usize(goal.payload.measure.len());
        for name in &goal.payload.measure {
            context.encode_name(&mut out, name)?;
        }
    }

    out.tag(9);
    out.usize(declaration.exports.len());
    for export in &declaration.exports {
        out.text(&export.kind);
        context.encode_name(&mut out, &export.name)?;
        out.bool(export.is_public);
    }

    if let Some(residuals) = package.residuals.get(&declaration.id) {
        out.tag(10);
        out.usize(residuals.len());
        for residual in residuals {
            out.u64(u64::from(residual.components));
            context.encode_expr(&mut out, residual.expr)?;
            out.usize(residual.algebraic.len());
            for name in &residual.algebraic {
                context.encode_name(&mut out, name)?;
            }
            out.usize(residual.rates.len());
            for name in &residual.rates {
                context.encode_name(&mut out, name)?;
            }
        }
    } else {
        out.tag(10);
        out.usize(0);
    }
    Ok(out.finish())
}

/// Versioned, length-framed canonical bytes for admitted meaning.
pub fn canonical_meaning_bytes(
    package: &SemanticPackage,
    dependencies: &[MeaningId],
) -> Result<Vec<u8>, MeaningError> {
    let aliases = alias_map(package);
    let mut declarations = package
        .declarations
        .iter()
        .map(|declaration| encode_declaration(package, declaration, &aliases))
        .collect::<Result<Vec<_>, _>>()?;
    declarations.sort();

    let mut dependency_ids = dependencies
        .iter()
        .map(MeaningId::as_str)
        .collect::<Vec<_>>();
    dependency_ids.sort_unstable();
    dependency_ids.dedup();

    let mut out = Encoder::default();
    out.text(MEANING_CANONICAL_SCHEMA_V1);
    out.usize(declarations.len());
    for declaration in declarations {
        out.blob(&declaration);
    }
    out.usize(dependency_ids.len());
    for dependency in dependency_ids {
        out.text(dependency);
    }
    Ok(out.finish())
}

/// Cryptographic identity of canonical admitted meaning.
pub fn meaning_id(
    package: &SemanticPackage,
    dependencies: &[MeaningId],
) -> Result<MeaningId, MeaningError> {
    canonical_meaning_bytes(package, dependencies).map(|bytes| MeaningId::from_bytes(&bytes))
}

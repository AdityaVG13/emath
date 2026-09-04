use super::{TestVerdict, definition_order};
use crate::interp::{EvalFault, Value, evaluate};
use crate::{lower_definition, lower_requirement};
use emath_ir::{
    BinaryOp, Declaration, ExprId, ExprNode, Literal, SemanticPackage, SliceAxis, TypeNode, UnaryOp,
};
use std::collections::BTreeMap;

pub(super) fn eval_givens(
    package: &SemanticPackage,
    test: &emath_ir::TestCase,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let mut given = BTreeMap::new();
    let mut seen: Vec<String> = Vec::new();
    let mut seen_values: Vec<Value> = Vec::new();
    for name in test.given.keys() {
        let expr = test.given[name];
        let program = lower_definition(package, expr, &seen, &[])
            .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
        match evaluate(&program, &seen_values, &[]) {
            Ok(value) => {
                given.insert(name.clone(), value.clone());
                seen.push(name.clone());
                seen_values.push(value);
            }
            Err(fault) => return Err(TestVerdict::Fault { fault }),
        }
    }
    Ok(given)
}

pub(super) fn eval_constructor(
    package: &SemanticPackage,
    declaration: &Declaration,
    constructor: &emath_ir::Constructor,
    given: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let param_names: Vec<String> = constructor
        .parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect();
    let mut param_values = Vec::with_capacity(param_names.len());
    for name in &param_names {
        let Some(value) = given.get(name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("test body does not supply constructor parameter `{name}`"),
            });
        };
        param_values.push(value);
    }

    for precondition in &constructor.preconditions {
        check_obligation(
            package,
            *precondition,
            &param_names,
            &param_values,
            "require",
        )?;
    }

    let mut state = BTreeMap::new();
    for field in &declaration.state {
        let Some(expr) = constructor.assignments.get(&field.name).copied() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("no `Self:` assignment for `{}`", field.name),
            });
        };
        let program = lower_definition(package, expr, &param_names, &[])
            .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
        match evaluate(&program, &param_values, &[]) {
            Ok(value) => {
                state.insert(field.name.clone(), value);
            }
            Err(fault) => return Err(TestVerdict::Fault { fault }),
        }
    }

    for postcondition in &constructor.postconditions {
        check_obligation(
            package,
            *postcondition,
            &param_names,
            &param_values,
            "ensure",
        )?;
    }
    Ok(state)
}

pub(super) fn check_obligation(
    package: &SemanticPackage,
    expr: ExprId,
    param_names: &[String],
    param_values: &[Value],
    keyword: &'static str,
) -> Result<(), TestVerdict> {
    let program = lower_requirement(package, expr, param_names)
        .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
    match evaluate(&program, param_values, &[]) {
        Ok(Value::Bool(true)) => Ok(()),
        Ok(Value::Bool(false)) => Err(TestVerdict::ConstructorRefused {
            obligation: format!("{keyword} {}", expr_text(package, expr)),
        }),
        Ok(Value::F64(_))
        | Ok(Value::I64(_))
        // Stage-2 (emath-t63iz): exact big values are not obligations.
        | Ok(Value::BigInt(_))
        | Ok(Value::BigVector(_))
        | Ok(Value::Rat { .. })
        | Ok(Value::Complex { .. })
        | Ok(Value::Vector(_))
        | Ok(Value::Matrix { .. })
        | Ok(Value::Tensor { .. })
        | Ok(Value::Interval { .. })
        | Ok(Value::Text(_))
        | Ok(Value::Series { .. })
        | Ok(Value::Set(_))
        | Ok(Value::Record { .. })
        | Ok(Value::List(_))
        | Ok(Value::Option(_))
        | Ok(Value::Result { .. })
        | Ok(Value::Program(_)) => Err(TestVerdict::Fault {
            fault: EvalFault::TypeConfusion {
                register: program.result.0,
                op: keyword,
            },
        }),
        Err(fault) => Err(TestVerdict::Fault { fault }),
    }
}

pub(super) fn eval_definitions(
    package: &SemanticPackage,
    declaration: &Declaration,
    given: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    eval_definitions_values(package, declaration, given, state)
}

pub(super) fn seed_state_from_given(
    declaration: &Declaration,
    given: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let mut state = BTreeMap::new();
    for field in &declaration.state {
        let Some(value) = given.get(&field.name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!(
                    "test body does not supply state `{name}`",
                    name = field.name
                ),
            });
        };
        state.insert(field.name.clone(), value);
    }
    Ok(state)
}

pub fn eval_definitions_values(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let input_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut bind_names = input_names;
    // Algebraic variables (implicit-residual unknowns) bind like inputs;
    // their guesses come from the same caller-supplied value map.
    if let Some(residuals) = package.residuals.get(&declaration.id) {
        if let Some(first) = residuals.first() {
            for name in &first.algebraic {
                if !bind_names.iter().any(|existing| existing == name) {
                    bind_names.push(name.clone());
                }
            }
        }
    }
    let mut bind_values = Vec::with_capacity(bind_names.len());
    for name in &bind_names {
        let Some(value) = inputs.get(name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("test body does not supply input `{name}`"),
            });
        };
        bind_values.push(coerce_to_slot(value, slot_ty(package, declaration, name)));
    }
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut state_values = Vec::with_capacity(state_names.len());
    for name in &state_names {
        let Some(value) = state.get(name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("missing state `{name}`"),
            });
        };
        state_values.push(coerce_to_slot(value, slot_ty(package, declaration, name)));
    }

    let mut definitions = BTreeMap::new();
    for (name, expr) in definition_order(package, declaration) {
        let program = lower_definition(package, expr, &bind_names, &state_names)
            .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
        match evaluate(&program, &bind_values, &state_values) {
            Ok(value) => {
                let value = coerce_to_slot(value, slot_ty(package, declaration, name));
                definitions.insert(name.clone(), value.clone());
                if !bind_names.iter().any(|existing| existing == name) {
                    bind_names.push(name.clone());
                    bind_values.push(value);
                }
            }
            Err(fault) => return Err(TestVerdict::Fault { fault }),
        }
    }
    Ok(definitions)
}

/// nothing-returns-nothing outcome: the definitions that computed, the
/// definitions returned as symbolic form, and the unbound names with
/// their declared types (constraints attached).
#[derive(Default)]
pub(super) struct SymbolicDefinitions {
    /// Definitions with an evaluable world, declaration-map order.
    pub definitions: BTreeMap<String, Value>,
    /// Definition name → symbolic form for definitions no world here can
    /// evaluate (they reference an unbound name or a symbolic result).
    pub forms: BTreeMap<String, String>,
    /// Unbound name → declared type (constraints attached).
    pub holes: BTreeMap<String, String>,
}

/// Symbolic lane of [`eval_definitions_values`]: definitions with an
/// evaluable world still compute; definitions that reference an unbound
/// name (or a symbolic definition) return their source-level form with
/// the meaning label carried by the verdict. Never a naked refusal for a
/// world gap; genuinely impossible lowering still refuses typed.
pub(super) fn eval_definitions_symbolic(
    package: &SemanticPackage,
    declaration: &Declaration,
    given: &BTreeMap<String, Value>,
) -> Result<SymbolicDefinitions, TestVerdict> {
    let mut holes: BTreeMap<String, String> = BTreeMap::new();
    let mut seen: Vec<String> = Vec::new();
    let mut seen_values: Vec<Value> = Vec::with_capacity(given.len());

    let mut bind_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    // Algebraic variables bind like inputs (mirrors the strict lane);
    // a missing one is a hole with its declared type.
    if let Some(residuals) = package.residuals.get(&declaration.id) {
        if let Some(first) = residuals.first() {
            for name in &first.algebraic {
                if !bind_names.iter().any(|existing| existing == name) {
                    bind_names.push(name.clone());
                }
            }
        }
    }
    for name in &bind_names {
        match given.get(name).cloned() {
            Some(value) => {
                seen.push(name.clone());
                seen_values.push(coerce_to_slot(value, slot_ty(package, declaration, name)));
            }
            None => {
                holes
                    .entry(name.clone())
                    .or_insert_with(|| hole_type_text(package, declaration, name));
            }
        }
    }

    // State is a hole unless every field can be seeded from the given
    // map; in this lane the constructor never ran.
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let state_seeded =
        !state_names.is_empty() && state_names.iter().all(|name| given.contains_key(name));
    let lowered_state_names: &[String] = if state_seeded {
        &state_names
    } else {
        for name in &state_names {
            holes
                .entry(name.clone())
                .or_insert_with(|| hole_type_text(package, declaration, name));
        }
        &[]
    };
    let state_values: Vec<Value> = if state_seeded {
        state_names
            .iter()
            .map(|name| coerce_to_slot(given[name].clone(), slot_ty(package, declaration, name)))
            .collect()
    } else {
        Vec::new()
    };

    let mut out = SymbolicDefinitions {
        definitions: BTreeMap::new(),
        forms: BTreeMap::new(),
        holes,
    };
    // A definition is symbolic when it references an unbound name or a
    // definition that itself came back symbolic (transitively).
    let mut symbolic_names: std::collections::BTreeSet<String> =
        out.holes.keys().cloned().collect();
    for (name, expr) in definition_order(package, declaration) {
        if expr_references(package, expr, &symbolic_names) {
            out.forms.insert(name.clone(), expr_text(package, expr));
            symbolic_names.insert(name.clone());
            continue;
        }
        let program = lower_definition(package, expr, &seen, lowered_state_names)
            .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
        match evaluate(&program, &seen_values, &state_values) {
            Ok(value) => {
                let value = coerce_to_slot(value, slot_ty(package, declaration, name));
                out.definitions.insert(name.clone(), value.clone());
                if !seen.iter().any(|existing| existing == name) {
                    seen.push(name.clone());
                    seen_values.push(value);
                }
            }
            Err(fault) => return Err(TestVerdict::Fault { fault }),
        }
    }
    Ok(out)
}

/// Whether the expression tree references any name in `unbound` (leaf
/// matching; binder variables shadow outer names of the same leaf in
/// their body, never in their own domain expression).
fn expr_references(
    package: &SemanticPackage,
    id: ExprId,
    unbound: &std::collections::BTreeSet<String>,
) -> bool {
    let Some(expr) = package.expr(id) else {
        return false;
    };
    match expr {
        ExprNode::Literal(_) | ExprNode::Series { .. } => false,
        ExprNode::Variable(name) => unbound.contains(name.leaf()),
        ExprNode::Call { arguments, .. } | ExprNode::Apply { arguments, .. } => arguments
            .iter()
            .any(|argument| expr_references(package, *argument, unbound)),
        ExprNode::Unary { value, .. } => expr_references(package, *value, unbound),
        ExprNode::Binary { left, right, .. } => {
            expr_references(package, *left, unbound) || expr_references(package, *right, unbound)
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            expr_references(package, *condition, unbound)
                || expr_references(package, *then_value, unbound)
                || expr_references(package, *else_value, unbound)
        }
        ExprNode::Record { fields, .. } => fields
            .values()
            .any(|field| expr_references(package, *field, unbound)),
        ExprNode::Index { value, indices } => {
            expr_references(package, *value, unbound)
                || indices
                    .iter()
                    .any(|index| expr_references(package, *index, unbound))
        }
        ExprNode::Slice { value, axes } => {
            expr_references(package, *value, unbound)
                || axes.iter().any(|axis| match axis {
                    SliceAxis::Point(point) => expr_references(package, *point, unbound),
                    SliceAxis::Range { start, end } => {
                        expr_references(package, *start, unbound)
                            || expr_references(package, *end, unbound)
                    }
                })
        }
        ExprNode::Binder {
            variables, body, ..
        } => {
            let inner: std::collections::BTreeSet<String> = unbound
                .iter()
                .filter(|name| !variables.iter().any(|variable| &variable.name == *name))
                .cloned()
                .collect();
            variables
                .iter()
                .any(|variable| expr_references(package, variable.domain, unbound))
                || expr_references(package, *body, &inner)
        }
        ExprNode::Set { elements, guards } => {
            elements
                .iter()
                .any(|element| expr_references(package, *element, unbound))
                || guards
                    .iter()
                    .filter_map(|guard| guard.as_ref())
                    .any(|guard| expr_references(package, *guard, unbound))
        }
        ExprNode::Vector(elements) => elements
            .iter()
            .any(|element| expr_references(package, *element, unbound)),
        ExprNode::Matrix(rows) => rows
            .iter()
            .flatten()
            .any(|element| expr_references(package, *element, unbound)),
        ExprNode::Tensor { elements, .. } => elements
            .iter()
            .any(|element| expr_references(package, *element, unbound)),
        ExprNode::Differentiate { body, .. } | ExprNode::Solve { body, .. } => {
            expr_references(package, *body, unbound)
        }
        ExprNode::Optimize { body, .. } => expr_references(package, *body, unbound),
        ExprNode::SampleLimit {
            body,
            target,
            direction,
            ..
        } => {
            expr_references(package, *body, unbound)
                || expr_references(package, *target, unbound)
                || expr_references(package, *direction, unbound)
        }
    }
}

/// Declared type of a bind slot, for hole constraints (`x: Float64`).
fn hole_type_text(package: &SemanticPackage, declaration: &Declaration, name: &str) -> String {
    slot_ty(package, declaration, name)
        .map(|ty| ty.display_name())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Widen I64→F64 (and whole F64→I64) so a named map matches typed slots.
pub(super) fn coerce_bindings(
    package: &SemanticPackage,
    declaration: &Declaration,
    values: &mut BTreeMap<String, Value>,
) {
    for (name, value) in values.iter_mut() {
        *value = coerce_to_slot(value.clone(), slot_ty(package, declaration, name));
    }
}

fn slot_ty<'a>(
    package: &'a SemanticPackage,
    declaration: &Declaration,
    name: &str,
) -> Option<&'a TypeNode> {
    declaration
        .inputs
        .iter()
        .chain(declaration.outputs.iter())
        .chain(declaration.state.iter())
        .chain(declaration.algebraic.iter())
        .find(|field| field.name == name)
        .and_then(|field| package.ty(field.ty))
        .or_else(|| {
            declaration.constructors.first().and_then(|ctor| {
                ctor.parameters
                    .iter()
                    .find(|field| field.name == name)
                    .and_then(|field| package.ty(field.ty))
            })
        })
}

fn coerce_to_slot(value: Value, ty: Option<&TypeNode>) -> Value {
    match (&value, ty) {
        (Value::I64(n), Some(TypeNode::Float64)) => Value::F64(*n as f64),
        (Value::F64(n), Some(TypeNode::Int | TypeNode::Nat))
            if n.is_finite()
                && n.fract() == 0.0
                && *n >= i64::MIN as f64
                && *n <= i64::MAX as f64 =>
        {
            Value::I64(*n as i64)
        }
        _ => value,
    }
}

pub(super) fn outputs_of(
    package: &SemanticPackage,
    declaration: &Declaration,
    definitions: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut outputs = BTreeMap::new();
    for field in &declaration.outputs {
        if let Some(value) = definitions.get(&field.name).cloned() {
            outputs.insert(
                field.name.clone(),
                coerce_to_slot(value, package.ty(field.ty)),
            );
        }
    }
    outputs
}

pub(super) fn eval_expect(
    package: &SemanticPackage,
    declaration: &Declaration,
    test: &emath_ir::TestCase,
    given: &BTreeMap<String, Value>,
    definitions: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> TestVerdict {
    let given_names: Vec<String> = given.keys().cloned().collect();
    let mut expect_names = given_names.clone();
    let mut expect_values: Vec<Value> = given.values().cloned().collect();
    for name in declaration.definitions.keys() {
        if given_names.iter().any(|given_name| given_name == name) {
            continue;
        }
        let Some(value) = definitions.get(name) else {
            continue;
        };
        expect_names.push(name.clone());
        expect_values.push(value.clone());
    }
    let Some(expect) = test.expect else {
        return TestVerdict::Computed;
    };
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let state_values: Vec<Value> = state_names
        .iter()
        .map(|name| state.get(name).cloned().unwrap_or(Value::F64(f64::NAN)))
        .collect();
    let program = match lower_definition(package, expect, &expect_names, &state_names) {
        Ok(program) => program,
        Err(detail) => return TestVerdict::LoweringRefused { detail },
    };
    match evaluate(&program, &expect_values, &state_values) {
        Ok(Value::Bool(true)) => TestVerdict::Passed,
        Ok(Value::Bool(false)) => TestVerdict::Failed,
        Ok(Value::F64(_))
        | Ok(Value::I64(_))
        // Stage-2 (emath-t63iz): exact big values are not obligations.
        | Ok(Value::BigInt(_))
        | Ok(Value::BigVector(_))
        | Ok(Value::Rat { .. })
        | Ok(Value::Complex { .. })
        | Ok(Value::Vector(_))
        | Ok(Value::Matrix { .. })
        | Ok(Value::Tensor { .. })
        | Ok(Value::Interval { .. })
        | Ok(Value::Text(_))
        | Ok(Value::Series { .. })
        | Ok(Value::Set(_))
        | Ok(Value::Record { .. })
        | Ok(Value::List(_))
        | Ok(Value::Option(_))
        | Ok(Value::Result { .. })
        | Ok(Value::Program(_)) => TestVerdict::Fault {
            fault: EvalFault::TypeConfusion {
                register: program.result.0,
                op: "expect",
            },
        },
        Err(fault) => TestVerdict::Fault { fault },
    }
}

fn expr_text(package: &SemanticPackage, id: ExprId) -> String {
    let Some(expr) = package.expr(id) else {
        return format!("<expr {}>", id.0);
    };
    match expr {
        ExprNode::Literal(Literal::Integer(text)) => text.clone(),
        ExprNode::Literal(Literal::FloatBits(bits)) => {
            crate::interp::format_f64(f64::from_bits(*bits))
        }
        ExprNode::Literal(Literal::Bool(flag)) => {
            if *flag {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        ExprNode::Literal(Literal::Text(text)) => format!("\"{text}\""),
        ExprNode::Literal(Literal::Rational(text)) => text.clone(),
        ExprNode::Variable(name) => name.0.clone(),
        ExprNode::Call {
            function,
            arguments,
        } => {
            let args: Vec<String> = arguments
                .iter()
                .map(|argument| expr_text(package, *argument))
                .collect();
            format!("{}({})", function.leaf(), args.join(", "))
        }
        ExprNode::Unary { operation, value } => {
            format!(
                "{}({})",
                unary_symbol(*operation),
                expr_text(package, *value)
            )
        }
        ExprNode::Binary {
            operation,
            left,
            right,
        } => match operation {
            BinaryOp::Min | BinaryOp::Max | BinaryOp::Atan2 => format!(
                "{}({}, {})",
                operation.name(),
                expr_text(package, *left),
                expr_text(package, *right)
            ),
            _ => format!(
                "{} {} {}",
                expr_text(package, *left),
                bin_symbol(*operation),
                expr_text(package, *right)
            ),
        },
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => format!(
            "if {} then {} else {}",
            expr_text(package, *condition),
            expr_text(package, *then_value),
            expr_text(package, *else_value)
        ),
        other => format!("{:?}", std::mem::discriminant(other)),
    }
}

fn unary_symbol(operation: UnaryOp) -> &'static str {
    match operation {
        UnaryOp::Negate => "-",
        UnaryOp::Not => "!",
        other => other.name(),
    }
}

fn bin_symbol(operation: BinaryOp) -> &'static str {
    match operation {
        BinaryOp::StrictFloatAdd => "+",
        BinaryOp::StrictFloatSub => "-",
        BinaryOp::StrictFloatMul => "*",
        BinaryOp::StrictFloatDiv => "/",
        BinaryOp::StrictFloatPow => "^",
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::Imply => "==>",
        BinaryOp::Iff => "<==>",
        other => other.name(),
    }
}

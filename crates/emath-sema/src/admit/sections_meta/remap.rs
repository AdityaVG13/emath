//! Arena id remapping from admitter-local to package-global index space.

use super::{ExprNode, ExprId, TypeId, SliceAxis};

/// Offset all child `ExprIds` and `TypeIds` in one node from the admitter's
/// local arena into the package's global index space.
pub(super) fn remap_expr_node(node: &mut ExprNode, expr_offset: u32, type_offset: u32) {
    let remap_e = |id: &mut ExprId| {
        id.0 += expr_offset;
    };
    let remap_t = |id: &mut TypeId| {
        id.0 += type_offset;
    };
    match node {
        ExprNode::Literal(_) | ExprNode::Variable(_) => {}
        // A series data constant carries no expr ids to remap
        // the pairs are inline f64s and the policy is text.
        ExprNode::Series { .. } => {}
        ExprNode::Call { arguments, .. } => {
            for id in arguments {
                remap_e(id);
            }
        }
        ExprNode::Unary { value, .. } => remap_e(value),
        ExprNode::Binary { left, right, .. } => {
            remap_e(left);
            remap_e(right);
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            remap_e(condition);
            remap_e(then_value);
            remap_e(else_value);
        }
        ExprNode::Record { ty, fields } => {
            remap_t(ty);
            for id in fields.values_mut() {
                remap_e(id);
            }
        }
        ExprNode::Index { value, indices } => {
            remap_e(value);
            for id in indices {
                remap_e(id);
            }
        }
        ExprNode::Slice { value, axes } => {
            remap_e(value);
            for axis in axes.iter_mut() {
                match axis {
                    SliceAxis::Point(id) => remap_e(id),
                    SliceAxis::Range { start, end } => {
                        remap_e(start);
                        remap_e(end);
                    }
                }
            }
        }
        ExprNode::Binder {
            variables, body, ..
        } => {
            for variable in variables {
                // Binder domains are ExprIds into the same local arena and
                // must be rebased with the rest of the graph (bug: function
                // N>1 variable-range binders pointed at function 1's nodes).
                remap_e(&mut variable.domain);
            }
            remap_e(body);
        }
        ExprNode::Vector(ids) => {
            for id in ids {
                remap_e(id);
            }
        }
        ExprNode::Set { elements, guards } => {
            for id in elements {
                remap_e(id);
            }
            for id in guards.iter_mut().flatten() {
                remap_e(id);
            }
        }
        ExprNode::Matrix(rows) => {
            for row in rows.iter_mut() {
                for id in row.iter_mut() {
                    remap_e(id);
                }
            }
        }
        ExprNode::Tensor { elements, .. } => {
            for id in elements {
                remap_e(id);
            }
        }
        ExprNode::Apply { arguments, .. } => {
            for id in arguments {
                remap_e(id);
            }
        }
        ExprNode::Program { body, .. } => remap_e(body),
    }
}

/// Offset all `ExprIds` and `TypeIds` in a declaration and its test cases into
/// the package's global index space.
pub(super) fn remap_ids(
    declaration: &mut emath_ir::Declaration,
    tests: &mut [emath_ir::constructor::TestCase],
    expr_offset: u32,
    type_offset: u32,
) {
    let remap_expr = |id: &mut ExprId| {
        id.0 += expr_offset;
    };
    let remap_type = |id: &mut TypeId| {
        id.0 += type_offset;
    };

    // Definitions
    for id in declaration.definitions.values_mut() {
        remap_expr(id);
    }
    // Invariants
    for id in &mut declaration.invariants {
        remap_expr(id);
    }
    // Inputs / outputs / state: Field ty
    for field in &mut declaration.inputs {
        remap_type(&mut field.ty);
    }
    for field in &mut declaration.outputs {
        remap_type(&mut field.ty);
    }
    for field in &mut declaration.state {
        remap_type(&mut field.ty);
    }
    // Constructors
    for ctor in &mut declaration.constructors {
        for id in &mut ctor.preconditions {
            remap_expr(id);
        }
        for id in ctor.assignments.values_mut() {
            remap_expr(id);
        }
        for id in &mut ctor.postconditions {
            remap_expr(id);
        }
        for id in ctor.defaults.values_mut() {
            remap_expr(id);
        }
        if let Some(id) = &mut ctor.error_type {
            remap_type(id);
        }
    }
    // Test cases: given bindings and the expected value.
    for test in tests.iter_mut() {
        for id in test.given.values_mut() {
            remap_expr(id);
        }
        if let Some(id) = &mut test.expect {
            remap_expr(id);
        }
    }
}

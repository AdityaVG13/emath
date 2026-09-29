//! Capability-cell consult helpers for the arithmetic lowering arms.

use super::{ExprId, Infer, E_UNSUPPORTED_TYPE, ExprNode};

impl super::super::Admitter {
    pub(super) fn capability_by_kernel(
        &self,
        kernel: &str,
    ) -> Option<super::super::CapabilityCallBinding> {
        self.capability_cells
            .iter()
            .find(|binding| binding.kernel.as_deref() == Some(kernel))
            .cloned()
    }

    pub(in crate::admit) fn apply_capability(
        &mut self,
        capability: &str,
        arguments: Vec<ExprId>,
        result: Infer,
        span: emath_core::Span,
    ) -> Option<(ExprId, Infer)> {
        let Some(capability) = self.capability_cells.iter()
            .find(|binding| binding.key == capability)
            .map(|binding| emath_ir::CapabilityId(binding.capability))
        else {
            self.error(E_UNSUPPORTED_TYPE, "capability is not executable in the loaded Language Image", span);
            return None;
        };
        let id = self.push_expr(
            ExprNode::Apply {
                capability,
                arguments,
            },
            span,
        );
        Some((id, result))
    }

    pub(super) fn apply_kernel(
        &mut self,
        kernel: &str,
        arguments: Vec<ExprId>,
        result: Infer,
        span: emath_core::Span,
    ) -> Option<(ExprId, Infer)> {
        let Some(binding) = self.capability_by_kernel(kernel) else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "capability kernel is not executable in the loaded Language Image",
                span,
            );
            return None;
        };
        let id = self.push_expr(
            ExprNode::Apply {
                capability: emath_ir::CapabilityId(binding.capability),
                arguments,
            },
            span,
        );
        Some((id, result))
    }
}

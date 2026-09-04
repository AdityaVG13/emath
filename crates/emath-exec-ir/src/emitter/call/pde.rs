//! Legacy dual-run emission for PDE operations whose carriers are not yet
//! representable by the generic capability ABI. Fixed 1D/2D calls resolve as
//! capsule applications before this module is reached.

use super::*;

impl super::super::Emitter {
    pub(super) fn emit_call_pde(
        &mut self,
        function: &str,
        package: &SemanticPackage,
        args: &[EmirExprRef],
        span: Span,
    ) -> Result<EmirValue, String> {
        match function {
            "laplacian_3d" | "laplacian_3d_neumann" => {
                if !matches!(args.len(), 2 | 4) {
                    return Err(format!(
                        "`{function}` expects (tensor, spacing) or (tensor, dx, dy, dz)"
                    ));
                }
                let input = self.emit(package, args[0])?;
                let spacing = if args.len() == 2 {
                    let h = positive_literal(package, args[1], function, "spacing")?;
                    [h, h, h]
                } else {
                    [
                        positive_literal(package, args[1], function, "dx")?,
                        positive_literal(package, args[2], function, "dy")?,
                        positive_literal(package, args[3], function, "dz")?,
                    ]
                };
                let inv = spacing.map(|h| 1.0 / (h * h));
                let weights = stencil3d_weights([
                    [inv[0], -2.0 * inv[0], inv[0]],
                    [inv[1], -2.0 * inv[1], inv[1]],
                    [inv[2], -2.0 * inv[2], inv[2]],
                ]);
                let edge = if function == "laplacian_3d_neumann" {
                    EdgePolicy::Neumann
                } else {
                    EdgePolicy::Clamp
                };
                self.push(
                    EmirOp::Stencil3d {
                        input,
                        weights,
                        center: (1, 1, 1),
                        edge,
                    },
                    span,
                )
            }
            "gradient_3d_x" | "gradient_3d_y" | "gradient_3d_z" => {
                if args.len() != 2 {
                    return Err(format!("`{function}` expects (tensor, spacing)"));
                }
                let input = self.emit(package, args[0])?;
                let spacing = positive_literal(package, args[1], function, "spacing")?;
                let axis = match function {
                    "gradient_3d_x" => 0,
                    "gradient_3d_y" => 1,
                    _ => 2,
                };
                self.push(
                    EmirOp::Stencil3d {
                        input,
                        weights: derivative3d_weights(axis, spacing),
                        center: (1, 1, 1),
                        edge: EdgePolicy::OneSided,
                    },
                    span,
                )
            }
            "div" | "div_3d" => {
                if !matches!(args.len(), 4 | 6) {
                    return Err(format!(
                        "`{function}` expects (vx, vy, vz, spacing) or (vx, vy, vz, dx, dy, dz)"
                    ));
                }
                let fields = [
                    self.emit(package, args[0])?,
                    self.emit(package, args[1])?,
                    self.emit(package, args[2])?,
                ];
                let spacing = if args.len() == 4 {
                    let h = positive_literal(package, args[3], function, "spacing")?;
                    [h, h, h]
                } else {
                    [
                        positive_literal(package, args[3], function, "dx")?,
                        positive_literal(package, args[4], function, "dy")?,
                        positive_literal(package, args[5], function, "dz")?,
                    ]
                };
                let mut derivatives = [EmirValue(0); 3];
                for axis in 0..3 {
                    derivatives[axis] = self.push(
                        EmirOp::Stencil3d {
                            input: fields[axis],
                            weights: derivative3d_weights(axis, spacing[axis]),
                            center: (1, 1, 1),
                            edge: EdgePolicy::OneSided,
                        },
                        span,
                    )?;
                }
                let xy = self.push(EmirOp::TensorAdd(derivatives[0], derivatives[1]), span)?;
                self.push(EmirOp::TensorAdd(xy, derivatives[2]), span)
            }
            _ => unreachable!("emit_call_pde routed a capsule-active operation"),
        }
    }
}

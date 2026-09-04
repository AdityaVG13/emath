//! Structural Option/Result carrier validation and lowering.

use emath_core::QualifiedName;

use super::*;

pub(super) fn carrier_arity(name: &str) -> Option<usize> {
    match name {
        "option_none" => Some(0),
        "option_some" | "option_is_some" | "result_ok" | "result_err" | "result_is_ok"
        | "result_error_of" => Some(1),
        "option_unwrap_or" | "result_unwrap_or" => Some(2),
        _ => None,
    }
}

impl super::super::super::Admitter {
    pub(super) fn lower_call_carriers(
        &mut self,
        name: &String,
        expr: &Expr,
        args: &[Expr],
    ) -> Option<(ExprId, Infer)> {
        match name.as_str() {
            "option_some" | "result_ok" | "result_err" => {
                let (payload_id, payload_infer) = self.lower_expr(&args[0])?;
                if !matches!(
                    payload_infer,
                    Infer::F64
                        | Infer::Nat
                        | Infer::Int
                        | Infer::Bool
                        | Infer::Complex
                        | Infer::HostDeferred
                        | Infer::OptionCarrier
                        | Infer::ResultCarrier
                ) {
                    self.error(
                        "E-TYPE-012",
                        format!("`{name}` payload must be a concrete scalar value or a nested Option/Result carrier"),
                        args[0].source,
                    );
                    return None;
                }
                let id = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName(name.clone()),
                        arguments: vec![payload_id],
                    },
                    expr.source,
                );
                let result = if name == "option_some" {
                    Infer::OptionCarrier
                } else {
                    Infer::ResultCarrier
                };
                Some((id, result))
            }
            "option_none" => {
                let id = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName(name.clone()),
                        arguments: Vec::new(),
                    },
                    expr.source,
                );
                Some((id, Infer::OptionCarrier))
            }
            "option_is_some" | "result_is_ok" => {
                let (carrier_id, carrier_infer) = self.lower_expr(&args[0])?;
                let admitted = match name.as_str() {
                    "option_is_some" => carrier_infer == Infer::OptionCarrier,
                    _ => carrier_infer == Infer::ResultCarrier,
                };
                if !admitted && carrier_infer != Infer::HostDeferred {
                    self.error(
                        "E-TYPE-012",
                        format!(
                            "`{name}` expects a {} carrier",
                            if matches!(name.as_str(), "option_is_some") {
                                "Option"
                            } else {
                                "Result"
                            }
                        ),
                        args[0].source,
                    );
                    return None;
                }
                let id = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName(name.clone()),
                        arguments: vec![carrier_id],
                    },
                    expr.source,
                );
                Some((id, Infer::Bool))
            }
            "option_unwrap_or" | "result_unwrap_or" => {
                let (carrier_id, carrier_infer) = self.lower_expr(&args[0])?;
                let carrier_ok = match name.as_str() {
                    "option_unwrap_or" => {
                        carrier_infer == Infer::OptionCarrier
                            || carrier_infer == Infer::HostDeferred
                    }
                    _ => {
                        carrier_infer == Infer::ResultCarrier
                            || carrier_infer == Infer::HostDeferred
                    }
                };
                if !carrier_ok {
                    self.error(
                        "E-TYPE-012",
                        format!("`{name}` expects an Option/Result carrier as its first argument"),
                        args[0].source,
                    );
                    return None;
                }
                let (default_id, default_infer) = self.lower_expr(&args[1])?;
                // Kind-matched default: the default's carrier, if
                // any, must be the SAME kind as the unwrapped
                // carrier (Option default for an Option, Result
                // default for a Result). A foreign carrier default
                // is a typed type-confusion, matching the term
                // layer's kind-specific guard.
                let same_kind_default = match name.as_str() {
                    "option_unwrap_or" => {
                        default_infer == Infer::OptionCarrier
                            || default_infer == Infer::HostDeferred
                    }
                    _ => {
                        default_infer == Infer::ResultCarrier
                            || default_infer == Infer::HostDeferred
                    }
                };
                if !matches!(
                    default_infer,
                    Infer::F64
                        | Infer::Nat
                        | Infer::Int
                        | Infer::Bool
                        | Infer::Complex
                        | Infer::HostDeferred
                ) && !same_kind_default
                {
                    self.error(
                        "E-TYPE-012",
                        format!("`{name}` default must be a concrete scalar value or a {} carrier (kind-matched)", if matches!(name.as_str(), "option_unwrap_or") { "Option" } else { "Result" }),
                        args[1].source,
                    );
                    return None;
                }
                let id = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName(name.clone()),
                        arguments: vec![carrier_id, default_id],
                    },
                    expr.source,
                );
                // A same-kind carrier default (like the option_none
                // used to extract a nested payload) yields a
                // carrier — the default's own Infer, type-honest.
                Some((id, default_infer))
            }
            "result_error_of" => {
                let (carrier_id, carrier_infer) = self.lower_expr(&args[0])?;
                if carrier_infer != Infer::ResultCarrier && carrier_infer != Infer::HostDeferred {
                    self.error(
                        "E-TYPE-012",
                        "`result_error_of` expects a Result carrier",
                        args[0].source,
                    );
                    return None;
                }
                let id = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName(name.clone()),
                        arguments: vec![carrier_id],
                    },
                    expr.source,
                );
                // Error-as-option: Err(x) → Some(x), Ok(_) → none.
                Some((id, Infer::OptionCarrier))
            }
            _ => unreachable!("lower_call_carriers routed a non-matching builtin"),
        }
    }
}

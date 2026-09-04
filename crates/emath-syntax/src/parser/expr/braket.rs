//! Dirac braket parsing: bras, kets, operands, inner products, sandwiches, projectors.

use super::*;

impl super::super::Parser {
    /// parse a bra-led braket form after `⟨` is consumed:
    /// `⟨φ|ψ⟩` (inner product), `⟨φ|P|ψ⟩` (sandwich), or the standalone
    /// bra `⟨φ|` (the conjugated vector; the conjugate is the identity
    /// on the pack's real carrier, so the bra desugars to its operand).
    pub(super) fn parse_bra_form(&mut self, start: Span, depth: usize) -> Option<Expr> {
        let bra = self.parse_braket_operand(start)?;
        if !self.eat(&TokenKind::Pipe) {
            self.error_here("E-SYN-101", "expected `|` to close the bra");
            return None;
        }
        match self.peek().clone() {
            // `⟨φ|ψ⟩` or `⟨φ|P|ψ⟩`: an operand follows the pipe.
            TokenKind::Int(_) | TokenKind::Ident(_) => {
                let mid = self.parse_braket_operand(start)?;
                match self.peek() {
                    TokenKind::RAngle => {
                        self.advance();
                        self.braket_inner(start, &bra, &mid)
                    }
                    TokenKind::Pipe => {
                        self.advance();
                        let ket = self.parse_braket_operand(start)?;
                        if !self.eat(&TokenKind::RAngle) {
                            self.error_here("E-SYN-101", "expected `⟩` to close the sandwich");
                            return None;
                        }
                        self.braket_sandwich(start, &bra, &mid, &ket)
                    }
                    other => {
                        self.error_here(
                            "E-SYN-101",
                            format!(
                                "expected `⟩` or `|` in the braket form, found {}",
                                other.describe()
                            ),
                        );
                        None
                    }
                }
            }
            // Standalone bra: end of the operand position.
            _ => self.braket_operand_expr(&bra, start),
        }
    }

    /// one braket operand — an integer basis label or an
    /// identifier naming a vector. Labels are validated when converted
    /// to values, so `⟨2|…⟩` refuses with the carrier named.
    pub(super) fn parse_braket_operand(&mut self, start: Span) -> Option<BraketOperand> {
        match self.peek().clone() {
            TokenKind::Int(label) => {
                self.advance();
                Some(BraketOperand::Label(label))
            }
            TokenKind::Ident(name) => {
                self.advance();
                let _ = start;
                Some(BraketOperand::Name(name))
            }
            other => {
                self.error_here(
                    "E-SYN-101",
                    format!(
                        "expected a braket label or name, found {}",
                        other.describe()
                    ),
                );
                None
            }
        }
    }

    /// the value of one braket operand. A label is the constant
    /// real basis vector on the pack's 2-level carrier; a name is the
    /// named vector itself.
    pub(super) fn braket_operand_expr(
        &mut self,
        operand: &BraketOperand,
        start: Span,
    ) -> Option<Expr> {
        match operand {
            BraketOperand::Label(label) => {
                let entries: [&str; 2] = match label.as_str() {
                    "0" => ["1.0", "0.0"],
                    "1" => ["0.0", "1.0"],
                    other => {
                        self.error_here(
                            "E-SYN-101",
                            format!(
                                "braket label `{other}` is outside the pack's real 2-level \
                                 carrier (|0⟩, |1⟩); a wider carrier (Complex entries, general \
                                 dimension) is a documented follow-up"
                            ),
                        );
                        return None;
                    }
                };
                Some(Expr {
                    kind: ExprKind::List(
                        entries
                            .iter()
                            .map(|text| Expr {
                                kind: ExprKind::Float((*text).into()),
                                source: start,
                            })
                            .collect(),
                    ),
                    source: start.cover(self.last_span()),
                })
            }
            BraketOperand::Name(name) => Some(Expr {
                kind: ExprKind::Path {
                    segments: vec![name.clone()],
                    generics: None,
                },
                source: start.cover(self.last_span()),
            }),
        }
    }

    /// `⟨φ|ψ⟩` — the inner product. Label×label folds to the
    /// Kronecker delta (`⟨0|1⟩` IS 0, machine-checked at parse);
    /// otherwise the exact desugar is the admitted `dot` builtin, since
    /// sesquilinear conjugation is the identity on the real carrier.
    pub(super) fn braket_inner(
        &mut self,
        start: Span,
        bra: &BraketOperand,
        ket: &BraketOperand,
    ) -> Option<Expr> {
        if let (BraketOperand::Label(a), BraketOperand::Label(b)) = (bra, ket) {
            let value = if a == b { "1" } else { "0" };
            return Some(Expr {
                kind: ExprKind::Int(value.into()),
                source: start.cover(self.last_span()),
            });
        }
        let bra_expr = self.braket_operand_expr(bra, start)?;
        let ket_expr = self.braket_operand_expr(ket, start)?;
        Some(Expr {
            kind: ExprKind::Call {
                function: Box::new(Expr {
                    kind: ExprKind::Path {
                        segments: vec!["dot".into()],
                        generics: None,
                    },
                    source: start,
                }),
                args: vec![bra_expr, ket_expr],
            },
            source: start.cover(self.last_span()),
        })
    }

    /// `⟨φ|P|ψ⟩` — the sandwich, as the double sum over the
    /// pack's 2-level carrier: `sum j in 0..2: φ[j] * (sum k in 0..2:
    /// P[j, k] * ψ[k])`. Every piece (sum binder, indexing, scalar
    /// multiply) is an admitted operation; the conjugate is the
    /// identity on the real carrier.
    pub(super) fn braket_sandwich(
        &mut self,
        start: Span,
        bra: &BraketOperand,
        mid: &BraketOperand,
        ket: &BraketOperand,
    ) -> Option<Expr> {
        let zero = || Expr {
            kind: ExprKind::Int("0".into()),
            source: start,
        };
        let two = || Expr {
            kind: ExprKind::Int("2".into()),
            source: start,
        };
        let carrier = || Expr {
            kind: ExprKind::Range {
                start: Some(Box::new(zero())),
                end: Some(Box::new(two())),
                inclusive: false,
            },
            source: start,
        };
        let binder_expr = |name: &str| Expr {
            kind: ExprKind::Path {
                segments: vec![name.into()],
                generics: None,
            },
            source: start,
        };
        let mid_expr = self.braket_operand_expr(mid, start)?;
        let ket_expr = self.braket_operand_expr(ket, start)?;
        let bra_expr = self.braket_operand_expr(bra, start)?;
        // inner = sum k in 0..2: P[j, k] * ψ[k]
        let inner_body = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Mul,
                left: Box::new(Expr {
                    kind: ExprKind::Index {
                        value: Box::new(mid_expr),
                        indices: vec![binder_expr("j"), binder_expr("k")],
                    },
                    source: start.cover(self.last_span()),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::Index {
                        value: Box::new(ket_expr),
                        indices: vec![binder_expr("k")],
                    },
                    source: start.cover(self.last_span()),
                }),
            },
            source: start.cover(self.last_span()),
        };
        let inner = Expr {
            kind: ExprKind::Binder {
                kind: BinderKind::Sum,
                binders: vec![Binder {
                    name: "k".into(),
                    domain: Some(carrier()),
                    source: start,
                }],
                body: Box::new(inner_body),
                guard: None,
            },
            source: start.cover(self.last_span()),
        };
        // outer = sum j in 0..2: φ[j] * inner
        let outer_body = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Mul,
                left: Box::new(Expr {
                    kind: ExprKind::Index {
                        value: Box::new(bra_expr),
                        indices: vec![binder_expr("j")],
                    },
                    source: start.cover(self.last_span()),
                }),
                right: Box::new(inner),
            },
            source: start.cover(self.last_span()),
        };
        Some(Expr {
            kind: ExprKind::Binder {
                kind: BinderKind::Sum,
                binders: vec![Binder {
                    name: "j".into(),
                    domain: Some(carrier()),
                    source: start,
                }],
                body: Box::new(outer_body),
                guard: None,
            },
            source: start.cover(self.last_span()),
        })
    }

    /// `|i⟩⟨j|` — the outer product (projector), the constant
    /// real matrix with 1 at `[i, j]` on the 2-level carrier.
    pub(super) fn braket_projector(&mut self, start: Span, row: &str, col: &str) -> Option<Expr> {
        let Ok(row_n) = row.parse::<usize>() else {
            self.error_here(
                "E-SYN-101",
                format!(
                    "braket label `{row}` is outside the pack's real 2-level carrier (|0⟩, |1⟩)"
                ),
            );
            return None;
        };
        let Ok(col_n) = col.parse::<usize>() else {
            self.error_here(
                "E-SYN-101",
                format!(
                    "braket label `{col}` is outside the pack's real 2-level carrier (|0⟩, |1⟩)"
                ),
            );
            return None;
        };
        if row_n > 1 || col_n > 1 {
            self.error_here(
                "E-SYN-101",
                format!(
                    "braket label outside the pack's real 2-level carrier (|0⟩, |1⟩): \
                     `|{row}⟩⟨{col}|`; a wider carrier is a documented follow-up"
                ),
            );
            return None;
        }
        let rows = (0..2)
            .map(|r| Expr {
                kind: ExprKind::List(
                    (0..2)
                        .map(|c| Expr {
                            kind: ExprKind::Float(
                                if r == row_n && c == col_n {
                                    "1.0"
                                } else {
                                    "0.0"
                                }
                                .into(),
                            ),
                            source: start,
                        })
                        .collect(),
                ),
                source: start,
            })
            .collect();
        Some(Expr {
            kind: ExprKind::List(rows),
            source: start.cover(self.last_span()),
        })
    }
}

pub(super) enum BraketOperand {
    Label(String),
    Name(String),
}

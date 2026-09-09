//! Literal CSV → Series admission. This is a compiler primitive: string
//! literals become `ExprNode::Series` data. Interpolation math stays in
//! the existing `SeriesSample` kernel, not in a named feature branch.

use emath_core::tree::{Expr, ExprKind};
use emath_core::QualifiedName;
use emath_ir::{ExprId, ExprNode};

use super::super::infer::Infer;

fn csv_series_column_name(header: &str) -> String {
    let header = header.trim();
    header
        .rfind('(')
        .filter(|_| header.ends_with(')'))
        .map_or(header, |unit_start| &header[..unit_start])
        .trim()
        .to_string()
}

/// RFC-4180-style CSV field splitter yielding one owned cell per field.
fn split_csv_fields(line: &str) -> (Vec<String>, bool) {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut malformed = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == ',' {
            fields.push(std::mem::take(&mut field));
        } else if ch == '"' {
            in_quotes = true;
        } else {
            field.push(ch);
        }
    }
    fields.push(field);
    if in_quotes {
        malformed = true;
    }
    (fields, malformed)
}

impl super::super::Admitter {
    pub(super) fn lower_series_from_csv(
        &mut self,
        args: &[Expr],
        expr: &Expr,
    ) -> Option<(ExprId, Infer)> {
        if args.len() != 5 {
            self.error(
                "E-TYPE-012",
                format!(
                    "`series_from_csv` expects CSV text, time column, value column, interpolation, and extrapolation; found {} arguments",
                    args.len()
                ),
                expr.source,
            );
            return None;
        }
        let Some(strings) = args
            .iter()
            .map(|argument| match &argument.kind {
                ExprKind::Str(value) => Some(value.as_str()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
        else {
            self.error(
                "E-SERIES-CSV",
                "`series_from_csv` arguments must be pure string literals",
                expr.source,
            );
            return None;
        };
        let [csv, time_column, value_column, interpolation, extrapolation] = strings.as_slice()
        else {
            unreachable!("arity checked above")
        };
        if !matches!(
            *interpolation,
            "previous" | "linear" | "nearest" | "pwc" | "monotone_cubic"
        ) {
            self.error(
                "E-SERIES-CSV",
                format!("unknown interpolation policy `{interpolation}`"),
                args[3].source,
            );
            return None;
        }
        if !matches!(*extrapolation, "refuse" | "clamp" | "extend") {
            self.error(
                "E-SERIES-CSV",
                format!("unknown extrapolation policy `{extrapolation}`"),
                args[4].source,
            );
            return None;
        }
        let mut lines = csv
            .lines()
            .map(|line| line.strip_prefix('\u{FEFF}').unwrap_or(line))
            .filter(|line| !line.trim().is_empty());
        let Some(header_line) = lines.next() else {
            self.error(
                "E-SERIES-CSV",
                "CSV input has no header row",
                args[0].source,
            );
            return None;
        };
        let (raw_header, header_malformed) = split_csv_fields(header_line);
        let header: Vec<String> = raw_header
            .iter()
            .map(|cell| csv_series_column_name(cell))
            .collect();
        if header_malformed {
            self.error(
                "E-CSV-006",
                "CSV header has an unclosed double-quote; fix or re-quote the header row",
                args[0].source,
            );
            return None;
        }
        let matching_column = |wanted: &str| -> (Option<usize>, usize) {
            let indices: Vec<usize> = (0..header.len())
                .filter(|&index| header[index] == wanted || raw_header[index].trim() == wanted)
                .collect();
            if indices.len() == 1 {
                (Some(indices[0]), indices.len())
            } else {
                (None, indices.len())
            }
        };
        let (time_index, time_count) = matching_column(time_column);
        let time_index = match time_count {
            0 => {
                self.error(
                    "E-CSV-001",
                    format!(
                        "time column `{time_column}` is missing; available columns: {}",
                        header.join(", ")
                    ),
                    args[1].source,
                );
                return None;
            }
            1 => time_index.expect("one match yields an index"),
            _ => {
                self.error(
                    "E-CSV-002",
                    format!(
                        "time column `{time_column}` is ambiguous: {time_count} columns match; available columns: {}",
                        header.join(", ")
                    ),
                    args[1].source,
                );
                return None;
            }
        };
        let (value_index, value_count) = matching_column(value_column);
        let value_index = match value_count {
            0 => {
                self.error(
                    "E-CSV-003",
                    format!(
                        "value column `{value_column}` is missing; available columns: {}",
                        header.join(", ")
                    ),
                    args[2].source,
                );
                return None;
            }
            1 => value_index.expect("one match yields an index"),
            _ => {
                self.error(
                    "E-CSV-004",
                    format!(
                        "value column `{value_column}` is ambiguous: {value_count} columns match; available columns: {}",
                        header.join(", ")
                    ),
                    args[2].source,
                );
                return None;
            }
        };
        let mut points = Vec::new();
        for (row_index, line) in lines.enumerate() {
            let (line_fields, row_malformed) = split_csv_fields(line);
            if row_malformed {
                self.error(
                    "E-CSV-006",
                    format!(
                        "CSV row {} has an unclosed double-quote; fix or re-quote the row",
                        row_index + 2
                    ),
                    args[0].source,
                );
                return None;
            }
            if line_fields.len() != header.len() {
                self.error(
                    "E-CSV-005",
                    format!(
                        "CSV row {} has {} cells, expected {}",
                        row_index + 2,
                        line_fields.len(),
                        header.len()
                    ),
                    args[0].source,
                );
                return None;
            }
            let parse_cell = |index: usize| {
                line_fields[index]
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
            };
            let (Some(time), Some(value)) = (parse_cell(time_index), parse_cell(value_index))
            else {
                self.error(
                    "E-CSV-008",
                    format!(
                        "CSV row {} selected columns must contain finite numbers",
                        row_index + 2
                    ),
                    args[0].source,
                );
                return None;
            };
            if points.last().is_some_and(|(previous, _)| time <= *previous) {
                self.error(
                    "E-CSV-009",
                    format!(
                        "CSV time column `{time_column}` is nonincreasing: row {} time {time} is not strictly after the previous row's time",
                        row_index + 2
                    ),
                    args[0].source,
                );
                return None;
            }
            points.push((time, value));
        }
        if points.is_empty() {
            self.error("E-CSV-007", "CSV series has no data rows", args[0].source);
            return None;
        }
        let id = self.push_expr(
            ExprNode::Series {
                points,
                interpolation: (*interpolation).to_string(),
                extrapolation: (*extrapolation).to_string(),
            },
            expr.source,
        );
        Some((id, Infer::Series))
    }

    pub(super) fn lower_series_at(
        &mut self,
        args: &[Expr],
        expr: &Expr,
    ) -> Option<(ExprId, Infer)> {
        if args.len() != 2 {
            self.error(
                "E-TYPE-012",
                format!(
                    "`series_at` expects a Series and a time coordinate; found {} arguments",
                    args.len()
                ),
                expr.source,
            );
            return None;
        }
        let (series_id, series_infer) = self.lower_expr(&args[0])?;
        if !matches!(series_infer, Infer::Series | Infer::HostDeferred) {
            self.error(
                "E-TYPE-012",
                "`series_at` expects a Series as its first argument",
                args[0].source,
            );
            return None;
        }
        let (time_id, time_infer) = self.lower_expr(&args[1])?;
        if !matches!(
            time_infer,
            Infer::F64 | Infer::Nat | Infer::Int | Infer::Unit { .. } | Infer::HostDeferred
        ) {
            self.error(
                "E-TYPE-012",
                "`series_at` expects a numeric time coordinate",
                args[1].source,
            );
            return None;
        }
        let id = self.push_expr(
            ExprNode::Call {
                function: QualifiedName("series_at".to_string()),
                arguments: vec![series_id, time_id],
            },
            expr.source,
        );
        Some((id, Infer::F64))
    }
}

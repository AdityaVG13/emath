//! Generic `emath feature` capsule admission.

use emath_core::tree::{CommandArgument, Declaration, Expr, ExprKind, StmtKind};
use emath_core::{Diagnostics, FeatureId, SemanticHash};
use emath_ir::{
    CapsuleEdge, CapsuleProjection, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity,
};
use std::collections::BTreeMap;
use std::str::FromStr;

use crate::admit::SemanticTrace;

pub(crate) fn admit_feature_capsule(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let errors_before = diagnostics.errors().count();
    let mut fields = BTreeMap::new();
    let mut edges = Vec::new();
    let mut projections = Vec::new();
    for stmt in &decl.body {
        let StmtKind::Command { head, argument } = &stmt.kind else {
            diagnostics.error(
                "E-CAPSULE-001",
                "Feature Capsule body accepts only data rows",
                stmt.source,
            );
            continue;
        };
        let key = head.join("_");
        let Some(value) = command_text(argument.as_ref()) else {
            diagnostics.error(
                "E-CAPSULE-001",
                format!("capsule row `{key}` requires a value"),
                stmt.source,
            );
            continue;
        };
        if key == "edge" {
            let Some((kind, target)) = value.split_once("->") else {
                diagnostics.error(
                    "E-CAPSULE-011",
                    "edge requires `kind -> FeatureID`",
                    stmt.source,
                );
                continue;
            };
            match FeatureId::from_str(target.trim()) {
                Ok(target) => edges.push(CapsuleEdge {
                    kind: kind.trim().to_string(),
                    target,
                }),
                Err(error) => diagnostics.error("E-CAPSULE-006", error.to_string(), stmt.source),
            }
        } else if key == "projection" {
            let Some((name, disposition)) = value.split_once("->") else {
                diagnostics.error(
                    "E-CAPSULE-013",
                    "projection requires `name -> disposition`",
                    stmt.source,
                );
                continue;
            };
            match emath_schema::parse_projection_disposition(disposition.trim()) {
                Ok(disposition) => projections.push(CapsuleProjection {
                    name: name.trim().to_string(),
                    disposition,
                }),
                Err(detail) => diagnostics.error("E-CAPSULE-013", detail, stmt.source),
            }
        } else if fields.insert(key.clone(), value).is_some() {
            diagnostics.error(
                "E-CAPSULE-003",
                format!("duplicate capsule row `{key}`"),
                stmt.source,
            );
        }
    }

    let required = |name: &str| fields.get(name).cloned();
    let parsed = (|| {
        let schema = required("schema")?;
        if schema != FEATURE_CAPSULE_SCHEMA {
            return None;
        }
        let feature_id = FeatureId::from_str(&required("feature_id")?).ok()?;
        let class = FeatureClass::from_str(&required("class")?).ok()?;
        feature_id.require_class(class.as_str()).ok()?;
        let semantic_hash = SemanticHash::from_str(&required("semantic_hash")?).ok()?;
        let maturity = Maturity::from_str(&required("maturity")?).ok()?;
        let summary = required("summary")?;
        let source = required("source")?;
        let mut slots = BTreeMap::new();
        for name in [
            "surface",
            "semantics",
            "exactness",
            "effects",
            "worlds",
            "providers",
            "artifacts",
            "reference",
            "conformance",
            "migration",
            "authority_target",
            "presentation",
            "agent",
        ] {
            let value = required(name)?;
            slots.insert(
                name.to_string(),
                emath_schema::parse_capsule_slot(&value).ok()?,
            );
        }
        Some(FeatureCapsule {
            schema,
            feature_id,
            semantic_hash,
            class,
            maturity,
            summary,
            source,
            edges,
            slots,
            projections,
        })
    })();
    let Some(capsule) = parsed else {
        diagnostics.error(
            "E-CAPSULE-004",
            "Feature Capsule has missing or invalid required rows",
            decl.head_source,
        );
        return;
    };
    let mut issues = Vec::new();
    emath_schema::validate_capsule(&capsule, &mut issues);
    for issue in issues {
        diagnostics.error(issue.code, issue.detail, decl.head_source);
    }
    if diagnostics.errors().count() == errors_before {
        package.feature_capsules.push(capsule);
        trace.record(
            "recognize:feature-capsule",
            format!("capsule `{}` admitted as candidate data", decl.name),
            Some(decl.head_source),
        );
    }
}

fn command_text(argument: Option<&CommandArgument>) -> Option<String> {
    match argument? {
        CommandArgument::Expr(expr) => expr_text(expr),
        CommandArgument::Assignment { name, value } => {
            expr_text(value).map(|value| format!("{name} = {value}"))
        }
        CommandArgument::List(values) => values
            .iter()
            .map(expr_text)
            .collect::<Option<Vec<_>>>()
            .map(|values| values.join(",")),
    }
}

fn expr_text(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Str(value) | ExprKind::Int(value) | ExprKind::Float(value) => Some(value.clone()),
        ExprKind::Bool(value) => Some(value.to_string()),
        ExprKind::Path {
            segments,
            generics: None,
        } => Some(segments.join(".")),
        _ => None,
    }
}

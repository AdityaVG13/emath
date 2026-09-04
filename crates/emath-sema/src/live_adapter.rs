//! Read-only source-to-artifact conformance introspection.

use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::{CanonicalField, FeatureId, OperationalHash, content_id_of_str};
use emath_ir::{ExprNode, Literal};

use crate::CompilerSession;

pub const LIVE_ADAPTER_SCHEMA: &str = "emath.live-conformance";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageStatus {
    Available(String),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveConformanceRequest<'a> {
    pub source_name: &'a str,
    pub source: &'a str,
    pub repository_commit: &'a str,
    pub compiler_identity: &'a str,
    pub language_image_id: &'a str,
    pub authority: &'a BTreeMap<FeatureId, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveConformanceResponse {
    pub schema: String,
    pub repository_commit: String,
    pub compiler_identity: String,
    pub language_image_id: String,
    pub source_hash: String,
    pub cst_identity: String,
    pub resolved_features: Vec<FeatureId>,
    pub stages: BTreeMap<String, StageStatus>,
    pub result_or_diagnosis: String,
    pub artifact_manifest: String,
    pub operational_hash: OperationalHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveAdapterError {
    InvalidCommit,
    InvalidImage,
    MissingAuthority(FeatureId),
    PartialClaim(String),
    MixedSource,
}

pub fn inspect_live_source(
    request: LiveConformanceRequest<'_>,
) -> Result<LiveConformanceResponse, LiveAdapterError> {
    if request.repository_commit.is_empty()
        || !request
            .repository_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(LiveAdapterError::InvalidCommit);
    }
    if !request
        .language_image_id
        .starts_with("distribution-sha256:")
    {
        return Err(LiveAdapterError::InvalidImage);
    }
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let (tree, parse_diagnostics) = session.parse_text(request.source);
    let checked = session.check_owned(request.source_name, request.source);
    let source_hash = content_id_of_str(request.source).0;
    let cst_identity = content_id_of_str(&format!("{tree:?}")).0;
    let mut features = infer_features(&checked.package);
    features.sort();
    features.dedup();
    for feature in &features {
        if !request.authority.contains_key(feature) {
            return Err(LiveAdapterError::MissingAuthority(feature.clone()));
        }
    }
    let mut stages = BTreeMap::new();
    stages.insert(
        "parse".to_string(),
        if parse_diagnostics.has_errors() {
            StageStatus::Unavailable("parse diagnostics".to_string())
        } else {
            StageStatus::Available(cst_identity.clone())
        },
    );
    stages.insert(
        "admit".to_string(),
        if checked.diagnostics.has_errors() {
            StageStatus::Unavailable(
                checked
                    .diagnostics
                    .errors()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>()
                    .join(","),
            )
        } else {
            StageStatus::Available(
                content_id_of_str(&format!("{:?}", checked.package.declarations)).0,
            )
        },
    );
    let lowering_available =
        !checked.package.declarations.is_empty() && !checked.diagnostics.has_errors();
    stages.insert(
        "lower".to_string(),
        if lowering_available {
            StageStatus::Available("neutral-sir".to_string())
        } else {
            StageStatus::Unavailable("no admitted runnable declaration".to_string())
        },
    );
    stages.insert(
        "world".to_string(),
        if lowering_available {
            StageStatus::Available("exact-int".to_string())
        } else {
            StageStatus::Unavailable("no world plan".to_string())
        },
    );
    let result = evaluate_tiny_exact(&checked.package).unwrap_or_else(|| {
        checked.diagnostics.errors().next().map_or_else(
            || "unavailable: execution stage has no supported tiny exact result".to_string(),
            |diagnostic| format!("diagnosis:{}", diagnostic.code),
        )
    });
    stages.insert(
        "execute".to_string(),
        if result.starts_with("value:") || result.starts_with("diagnosis:") {
            StageStatus::Available(result.clone())
        } else {
            StageStatus::Unavailable(result.clone())
        },
    );
    let artifact_manifest = format!(
        "schema=emath.live-artifact\nsource_hash={source_hash}\nimage={}\nresult={}\n",
        request.language_image_id, result
    );
    stages.insert(
        "artifact".to_string(),
        StageStatus::Available(content_id_of_str(&artifact_manifest).0),
    );
    let operational_hash = OperationalHash::new(&[
        CanonicalField::new("repository_commit", request.repository_commit.as_bytes())
            .expect("fixed"),
        CanonicalField::new("binary_identity", request.compiler_identity.as_bytes())
            .expect("fixed"),
    ])
    .expect("operational fields are valid");
    Ok(LiveConformanceResponse {
        schema: LIVE_ADAPTER_SCHEMA.to_string(),
        repository_commit: request.repository_commit.to_string(),
        compiler_identity: request.compiler_identity.to_string(),
        language_image_id: request.language_image_id.to_string(),
        source_hash,
        cst_identity,
        resolved_features: features,
        stages,
        result_or_diagnosis: result,
        artifact_manifest,
        operational_hash,
    })
}

impl LiveConformanceResponse {
    pub fn validate(&self) -> Result<(), LiveAdapterError> {
        for required in ["parse", "admit", "lower", "world", "execute", "artifact"] {
            if !self.stages.contains_key(required) {
                return Err(LiveAdapterError::PartialClaim(required.to_string()));
            }
        }
        if !self
            .artifact_manifest
            .contains(&format!("source_hash={}", self.source_hash))
            || !self
                .artifact_manifest
                .contains(&format!("image={}", self.language_image_id))
            || !self
                .artifact_manifest
                .contains(&format!("result={}", self.result_or_diagnosis))
        {
            return Err(LiveAdapterError::MixedSource);
        }
        Ok(())
    }
}

fn infer_features(package: &emath_ir::SemanticPackage) -> Vec<FeatureId> {
    let mut features = Vec::new();
    for declaration in &package.declarations {
        if let Ok(id) = FeatureId::from_str("std.kind.function") {
            features.push(id);
        }
        for expression in declaration.definitions.values() {
            collect_expr(package, *expression, &mut features);
        }
    }
    features
}

fn collect_expr(
    package: &emath_ir::SemanticPackage,
    id: emath_ir::ExprId,
    features: &mut Vec<FeatureId>,
) {
    let Some(expression) = package.exprs.get(id.index()) else {
        return;
    };
    match expression {
        ExprNode::Literal(Literal::Integer(_)) => {
            features.push(FeatureId::from_str("std.type.int").unwrap())
        }
        ExprNode::Binary {
            operation: emath_ir::BinaryOp::ExactAdd | emath_ir::BinaryOp::StrictFloatAdd,
            left,
            right,
        } => {
            features.push(FeatureId::from_str("std.capability.math.add").unwrap());
            collect_expr(package, *left, features);
            collect_expr(package, *right, features);
        }
        _ => {}
    }
}

fn evaluate_tiny_exact(package: &emath_ir::SemanticPackage) -> Option<String> {
    for declaration in &package.declarations {
        for expression in declaration.definitions.values() {
            if let Some(value) = eval_int(package, *expression) {
                return Some(format!("value:{value}:exact-int"));
            }
        }
    }
    None
}

fn eval_int(package: &emath_ir::SemanticPackage, id: emath_ir::ExprId) -> Option<i64> {
    match package.exprs.get(id.index())? {
        ExprNode::Literal(Literal::Integer(value)) => value.parse().ok(),
        ExprNode::Binary {
            operation: emath_ir::BinaryOp::ExactAdd | emath_ir::BinaryOp::StrictFloatAdd,
            left,
            right,
        } => eval_int(package, *left)?.checked_add(eval_int(package, *right)?),
        ExprNode::Literal(Literal::FloatBits(bits)) => {
            let value = f64::from_bits(*bits);
            (value.fract() == 0.0).then_some(value as i64)
        }
        _ => None,
    }
}

//! JSON field helpers and enum round-trips.

use super::*;

pub(super) fn problem(code: &'static str, message: &str) -> LabProblem {
    LabProblem {
        code,
        message: message.to_string(),
    }
}

pub(super) fn artifact_json(artifact: &ArtifactRef) -> JsonValue {
    JsonValue::Object(vec![
        (
            "package".into(),
            JsonValue::String(artifact.package.clone()),
        ),
        (
            "content_id".into(),
            JsonValue::String(artifact.content_id.0.clone()),
        ),
        (
            "profile".into(),
            JsonValue::String(artifact.profile.clone()),
        ),
    ])
}

pub(super) fn artifact_from_json(value: &JsonValue, at: &str) -> Result<ArtifactRef, LabError> {
    let object = expect_object(value, at)?;
    Ok(ArtifactRef {
        package: expect_string(field(object, "package")?, &format!("{at}.package"))?.to_string(),
        content_id: ContentId(
            expect_string(field(object, "content_id")?, &format!("{at}.content_id"))?.to_string(),
        ),
        profile: expect_string(field(object, "profile")?, &format!("{at}.profile"))?.to_string(),
    })
}

pub(super) fn field<'a>(object: &'a [(String, JsonValue)], name: &str) -> Result<&'a JsonValue, LabError> {
    object
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| {
            LabError::new(
                "E-HOST-003",
                format!("manifest JSON is missing field {name}"),
            )
        })
}

pub(super) fn expect_object<'a>(
    value: &'a JsonValue,
    at: &str,
) -> Result<&'a [(String, JsonValue)], LabError> {
    match value {
        JsonValue::Object(fields) => Ok(fields),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON object"),
        )),
    }
}

pub(super) fn expect_array<'a>(value: &'a JsonValue, at: &str) -> Result<&'a [JsonValue], LabError> {
    match value {
        JsonValue::Array(items) => Ok(items),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON array"),
        )),
    }
}

pub(super) fn expect_string<'a>(value: &'a JsonValue, at: &str) -> Result<&'a str, LabError> {
    match value {
        JsonValue::String(text) => Ok(text),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON string"),
        )),
    }
}

pub(super) fn expect_number(value: &JsonValue, at: &str) -> Result<f64, LabError> {
    match value {
        JsonValue::Number(number) => Ok(*number),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON number"),
        )),
    }
}

/// Non-negative integer field; rejects fractional, negative, NaN, and
/// out-of-range values instead of silently truncating.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn expect_u64(value: &JsonValue, at: &str) -> Result<u64, LabError> {
    let number = expect_number(value, at)?;
    if number.is_finite() && number.fract() == 0.0 && number >= 0.0 && number < 2_f64.powi(64) {
        Ok(number as u64)
    } else {
        Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a non-negative integer"),
        ))
    }
}

/// Serializes an integer count as a JSON number; counts are practical
/// magnitudes, so the `u64 -> f64` conversion is exact in practice.
#[allow(clippy::cast_precision_loss)]
pub(super) fn json_count(count: u64) -> JsonValue {
    JsonValue::Number(count as f64)
}

pub(super) fn expect_bool(value: &JsonValue, at: &str) -> Result<bool, LabError> {
    match value {
        JsonValue::Bool(flag) => Ok(*flag),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON boolean"),
        )),
    }
}

pub(super) fn parse_partition_kind(token: &str) -> Result<PartitionKind, LabError> {
    match token {
        "training" => Ok(PartitionKind::Training),
        "calibration" => Ok(PartitionKind::Calibration),
        "validation" => Ok(PartitionKind::Validation),
        "holdout" => Ok(PartitionKind::Holdout),
        "stress" => Ok(PartitionKind::Stress),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown partition kind {other}"),
        )),
    }
}

pub(super) fn parse_direction(token: &str) -> Result<MetricDirection, LabError> {
    match token {
        "lower" => Ok(MetricDirection::LowerIsBetter),
        "higher" => Ok(MetricDirection::HigherIsBetter),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown metric direction {other}"),
        )),
    }
}

pub(super) fn parse_kill_action(token: &str) -> Result<KillAction, LabError> {
    match token {
        "declare_incumbent" => Ok(KillAction::DeclareIncumbent),
        "cancel_run" => Ok(KillAction::CancelRun),
        "quarantine_candidate" => Ok(KillAction::QuarantineCandidate),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown kill action {other}"),
        )),
    }
}

pub(super) fn parse_fallback_action(token: &str) -> Result<FallbackAction, LabError> {
    match token {
        "retain_baseline" => Ok(FallbackAction::RetainBaseline),
        "shadow_candidate" => Ok(FallbackAction::ShadowCandidate),
        "quarantine_candidate" => Ok(FallbackAction::QuarantineCandidate),
        "diagnostic" => Ok(FallbackAction::ExplicitDiagnostic),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown fallback action {other}"),
        )),
    }
}

pub(super) fn kill_condition_token(condition: &KillCondition) -> String {
    match condition {
        KillCondition::CorrectnessFailure => "correctness".to_string(),
        KillCondition::EvidenceMissing => "evidence_missing".to_string(),
        KillCondition::RegressionBelow { median_ratio } => {
            format!("regression_below:{median_ratio}")
        }
        KillCondition::MemoryOver { bytes } => format!("memory_over:{bytes}"),
        KillCondition::HangOver { seconds } => format!("hang_over:{seconds}"),
    }
}

pub(super) fn kill_condition_json(condition: &KillCondition) -> JsonValue {
    match condition {
        KillCondition::CorrectnessFailure => JsonValue::Object(vec![(
            "kind".into(),
            JsonValue::String("correctness".into()),
        )]),
        KillCondition::EvidenceMissing => JsonValue::Object(vec![(
            "kind".into(),
            JsonValue::String("evidence_missing".into()),
        )]),
        KillCondition::RegressionBelow { median_ratio } => JsonValue::Object(vec![
            ("kind".into(), JsonValue::String("regression_below".into())),
            ("median_ratio".into(), JsonValue::Number(*median_ratio)),
        ]),
        KillCondition::MemoryOver { bytes } => JsonValue::Object(vec![
            ("kind".into(), JsonValue::String("memory_over".into())),
            ("bytes".into(), json_count(*bytes)),
        ]),
        KillCondition::HangOver { seconds } => JsonValue::Object(vec![
            ("kind".into(), JsonValue::String("hang_over".into())),
            ("seconds".into(), json_count(*seconds)),
        ]),
    }
}

pub(super) fn kill_condition_from_json(value: &JsonValue) -> Result<KillCondition, LabError> {
    let object = expect_object(value, "kill_rule.condition")?;
    let kind = expect_string(field(object, "kind")?, "kill_rule.condition.kind")?;
    match kind {
        "correctness" => Ok(KillCondition::CorrectnessFailure),
        "evidence_missing" => Ok(KillCondition::EvidenceMissing),
        "regression_below" => Ok(KillCondition::RegressionBelow {
            median_ratio: expect_number(
                field(object, "median_ratio")?,
                "kill_rule.condition.median_ratio",
            )?,
        }),
        "memory_over" => Ok(KillCondition::MemoryOver {
            bytes: expect_u64(field(object, "bytes")?, "kill_rule.condition.bytes")?,
        }),
        "hang_over" => Ok(KillCondition::HangOver {
            seconds: expect_u64(field(object, "seconds")?, "kill_rule.condition.seconds")?,
        }),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown kill condition {other}"),
        )),
    }
}

/// Token for a frozen statistical protocol reference (lives in `stats`).
#[must_use]
pub fn protocol_token(protocol: &StatisticalProtocol) -> String {
    format!(
        "warm={}:reps={}:min={}:paired={}:seed={}:outlier={}:retain={}:randomize={}",
        protocol.warmup_repetitions,
        protocol.repetitions,
        protocol.min_repetitions,
        protocol.paired,
        protocol.seed,
        outlier_token(&protocol.outlier),
        protocol.retain_raw,
        protocol.randomize_order
    )
}

pub(super) fn outlier_token(outlier: &crate::stats::OutlierPolicy) -> String {
    match outlier {
        crate::stats::OutlierPolicy::KeepAll => "keep-all".to_string(),
        crate::stats::OutlierPolicy::MadTrim { factor } => format!("mad-trim:{factor}"),
    }
}

//! Library-backed Language Image inspection commands.

use std::collections::BTreeMap;

use emath_artifact::{AuthorityLock, AuthorityReceipt};
use emath_core::FeatureId;
use emath_exec_ir::language_image::LanguageImage;
use emath_ir::{FeatureCapsule, MeaningResource, MeaningSpine};

pub const LANGUAGE_INSPECTION_SCHEMA: &str = "emath.language-inspection";

#[derive(Clone, Debug)]
pub struct LanguageInspection<'a> {
    pub image: &'a LanguageImage,
    pub capsules: &'a [FeatureCapsule],
    pub spine: &'a MeaningSpine,
    pub authority: &'a AuthorityLock,
    pub receipts: &'a BTreeMap<FeatureId, AuthorityReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageCommand {
    Orient(FeatureId),
    Impact(FeatureId),
    Authority(FeatureId),
    Gaps(Option<String>),
    CheckImage,
    Receipt(FeatureId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageInspectError {
    StaleImage,
    UnknownFeature(FeatureId),
    HiddenHole(FeatureId),
    IncompleteReceipt(FeatureId),
}

impl LanguageInspection<'_> {
    pub fn run(
        &self,
        command: LanguageCommand,
        json: bool,
    ) -> Result<String, LanguageInspectError> {
        self.image
            .verify()
            .map_err(|_| LanguageInspectError::StaleImage)?;
        let body = match command {
            LanguageCommand::Orient(id) => self.orient(&id)?,
            LanguageCommand::Impact(id) => self.impact(&id)?,
            LanguageCommand::Authority(id) => self.authority(&id)?,
            LanguageCommand::Gaps(scope) => self.gaps(scope.as_deref()),
            LanguageCommand::CheckImage => {
                format!("image={} status=fresh\n", self.image.distribution_hash)
            }
            LanguageCommand::Receipt(id) => self.receipt(&id)?,
        };
        if json {
            Ok(format!(
                "{{\"schema\":\"{LANGUAGE_INSPECTION_SCHEMA}\",\"image_id\":\"{}\",\"output\":\"{}\"}}\n",
                self.image.distribution_hash,
                escape(&body)
            ))
        } else {
            Ok(format!(
                "image_id={} fresh=true\n{body}",
                self.image.distribution_hash
            ))
        }
    }

    fn capsule(&self, id: &FeatureId) -> Result<&FeatureCapsule, LanguageInspectError> {
        self.capsules
            .iter()
            .find(|capsule| &capsule.feature_id == id)
            .ok_or_else(|| LanguageInspectError::UnknownFeature(id.clone()))
    }

    fn orient(&self, id: &FeatureId) -> Result<String, LanguageInspectError> {
        let capsule = self.capsule(id)?;
        let context = self.spine.minimum_agent_context(capsule);
        Ok(format!(
            "feature={} class={} maturity={} source={} owner={} hazards={} direct={} conformance={} migrations={}\n",
            id,
            capsule.class,
            capsule.maturity.as_str(),
            capsule.source,
            context.owner_contract,
            context.hazards,
            resources(&context.direct_dependencies),
            resources(&context.conformance),
            resources(&context.migrations)
        ))
    }

    fn impact(&self, id: &FeatureId) -> Result<String, LanguageInspectError> {
        self.capsule(id)?;
        let impact = self
            .spine
            .reverse_impact(&MeaningResource::Feature(id.clone()));
        if impact.is_empty() {
            return Err(LanguageInspectError::UnknownFeature(id.clone()));
        }
        Ok(format!("feature={id} impact={}\n", resources(&impact)))
    }

    fn authority(&self, id: &FeatureId) -> Result<String, LanguageInspectError> {
        let capsule = self.capsule(id)?;
        if capsule.has_blocking_hole() {
            return Err(LanguageInspectError::HiddenHole(id.clone()));
        }
        let entry = self
            .authority
            .entries
            .get(id)
            .ok_or_else(|| LanguageInspectError::UnknownFeature(id.clone()))?;
        Ok(format!(
            "feature={id} maturity={} authority={} active_source={} semantic_hash={} holes=none\n",
            capsule.maturity.as_str(),
            entry.state.as_str(),
            entry.active_source,
            entry.semantic_hash
        ))
    }

    fn gaps(&self, scope: Option<&str>) -> String {
        let mut output = String::new();
        for capsule in self.capsules {
            if scope.is_some_and(|scope| !capsule.feature_id.as_str().contains(scope)) {
                continue;
            }
            let active = self
                .authority
                .entries
                .get(&capsule.feature_id)
                .is_some_and(|entry| entry.state == emath_artifact::AuthorityState::CapsuleActive);
            if !active || capsule.has_blocking_hole() {
                output.push_str(&format!(
                    "feature={} maturity={} active={} next={}\n",
                    capsule.feature_id,
                    capsule.maturity.as_str(),
                    active,
                    if capsule.has_blocking_hole() {
                        "resolve-hole"
                    } else {
                        "complete-publication"
                    }
                ));
            }
        }
        output
    }

    fn receipt(&self, id: &FeatureId) -> Result<String, LanguageInspectError> {
        let receipt = self
            .receipts
            .get(id)
            .ok_or_else(|| LanguageInspectError::IncompleteReceipt(id.clone()))?;
        if receipt.conformance.is_empty()
            || receipt.generated_views.is_empty()
            || receipt.rollback.is_empty()
        {
            return Err(LanguageInspectError::IncompleteReceipt(id.clone()));
        }
        Ok(format!(
            "{}reproduce=cargo test -p owning-package --test feature-contract\n",
            receipt.canonical()
        ))
    }
}

fn resources(values: &[MeaningResource]) -> String {
    values
        .iter()
        .map(MeaningResource::canonical)
        .collect::<Vec<_>>()
        .join(",")
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

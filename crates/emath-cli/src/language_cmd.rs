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

/// Compact discovery uses the verified Language Image, never catalog counts
/// as evidence that a mathematical operation executes locally.
#[derive(Clone, Debug)]
pub(crate) struct ApiRequest {
    search: String,
    offset: usize,
    limit: usize,
    source: Option<std::path::PathBuf>,
    json: bool,
}

impl ApiRequest {
    pub(crate) fn parse(args: &[String]) -> Option<Self> {
        let mut search = None;
        let mut offset = None;
        let mut limit = None;
        let mut source = None;
        let mut json = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--search" => {
                    crate::assign_once(
                        &mut search,
                        crate::take_nonflag_value(args, &mut index)?.to_string(),
                    )?;
                }
                "--offset" => {
                    crate::assign_once(
                        &mut offset,
                        crate::take_nonflag_value(args, &mut index)?
                            .parse::<usize>()
                            .ok()?,
                    )?;
                }
                "--limit" => {
                    let value = crate::take_nonflag_value(args, &mut index)?
                        .parse::<usize>()
                        .ok()?;
                    if !(1..=128).contains(&value) {
                        return None;
                    }
                    crate::assign_once(&mut limit, value)?;
                }
                "--source" => {
                    crate::assign_once(
                        &mut source,
                        std::path::PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                    )?;
                }
                "--json" => json = true,
                _ => return None,
            }
            index += 1;
        }
        Some(Self {
            search: search.unwrap_or_default(),
            offset: offset.unwrap_or(0),
            limit: limit.unwrap_or(8),
            source,
            json,
        })
    }
}

pub(crate) fn api(request: ApiRequest) -> crate::CliExit {
    let distribution = match crate::cli_dispatch::load_verified_language(request.source.as_deref())
    {
        Ok(distribution) => distribution,
        Err(error) => {
            return crate::execution::diagnostic(
                request.json,
                crate::EXIT_REFUSED,
                "E-LANG-IMAGE",
                &error,
            );
        }
    };
    let mut matches: Vec<_> = distribution
        .capsules
        .iter()
        .filter(|capsule| capsule.feature_id.as_str().contains(&request.search))
        .collect();
    matches.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    let mut features = Vec::new();
    for capsule in matches.iter().skip(request.offset).take(request.limit) {
        let authority = distribution.authority.entries.get(&capsule.feature_id);
        let active = authority.is_some_and(|entry| entry.state.as_str() == "capsule-active");
        let kernel =
            emath_exec_ir::native_kernel::verified_kernel_binding(capsule.feature_id.as_str()).ok();
        let reference = distribution
            .reference_programs
            .contains_key(&capsule.feature_id);
        let execution = if !active {
            "not-active"
        } else if kernel.is_some() {
            "executable-native"
        } else if reference {
            "executable-reference"
        } else if capsule.class == emath_ir::FeatureClass::Capability {
            "not-executable-locally"
        } else {
            "language-surface"
        };
        let mut row = emath_artifact::JsonWriter::object();
        row.string("feature_id", capsule.feature_id.as_str());
        row.string("class", capsule.class.as_str());
        row.string("maturity", capsule.maturity.as_str());
        row.string("availability", execution);
        row.string("source", &capsule.source);
        if let Some(kernel) = kernel {
            row.string("signature", &kernel.signature);
        }
        for name in ["surface", "semantics"] {
            if let Some(emath_ir::CapsuleSlot::Value(value)) = capsule.slots.get(name) {
                // Omission is explicit; the full authored source remains
                // the authority, not a shortened discovery description.
                row.string(name, &value.chars().take(512).collect::<String>());
                if value.chars().count() > 512 {
                    row.bool(&format!("{name}_truncated"), true);
                }
            }
        }
        if !request.json {
            println!(
                "{} [{}]\n  source: {}",
                capsule.feature_id, execution, capsule.source
            );
        }
        features.push(row.finish());
    }
    let mut commands = Vec::new();
    for name in [
        "api", "new", "check", "run", "step", "inspect", "verify", "build", "simulate",
    ] {
        let mut command = emath_artifact::JsonWriter::object();
        command.string("name", name);
        command.string("usage", crate::catalog::command_usage(name).unwrap_or(name));
        command.string(
            "summary",
            crate::catalog::command_summary(name).unwrap_or(""),
        );
        commands.push(command.finish());
    }
    let next = request.offset.saturating_add(features.len());
    if request.json {
        let mut out = emath_artifact::JsonWriter::object();
        out.string("schema", "emath.api.v1");
        out.string("version", env!("CARGO_PKG_VERSION"));
        out.string(
            "language_id",
            &distribution.image.distribution_hash.to_string(),
        );
        out.string("source_of_truth", "language/");
        out.string(
            "starter_source",
            "emath function Answer:\n    definitions:\n        result = 2 + 1\n",
        );
        out.strings(
            "workflow",
            &[
                "emath check program.emath --json".into(),
                "emath run program.emath --json".into(),
            ],
        );
        out.string(
            "run_results",
            "emath.run.v1: typed values, evidence scope, goal_met, remaining cases and checkpoint",
        );
        out.string(
            "continuation",
            "completed examples/declaration calls only; no opaque solver-internal suspension",
        );
        out.string("parameter_values", "--set supports Float64, Int, Nat, BigInt, Bool and numeric vectors; other carriers use source examples");
        out.objects("commands", &commands);
        out.string("search", &request.search);
        out.int("offset", request.offset as u64);
        out.int("matching_features", matches.len() as u64);
        out.bool("truncated", next < matches.len());
        if next < matches.len() {
            out.int("next_offset", next as u64);
        }
        out.objects("features", &features);
        println!("{}", out.finish());
    } else {
        println!(
            "{} matching features; showing {}..{}",
            matches.len(),
            request.offset.min(matches.len()),
            next.min(matches.len())
        );
        println!("write program.emath -> emath check program.emath -> emath run program.emath");
        println!("Use --json for command arguments, result contracts and the starter source.");
        if next < matches.len() {
            println!("More results: add --offset {next} with the same --search.");
        }
    }
    crate::EXIT_OK
}

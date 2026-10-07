//! `emath api`: discovery over the verified Language Image.

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
    let roots = emath_exec_ir::constructor_layer::module_roots_for(request.source.as_deref());
    let mut ranked: Vec<_> = emath_exec_ir::module_index::module_exports(&roots)
        .into_iter()
        .filter_map(|export| export.rank(&request.search).map(|rank| (rank, export)))
        .collect();
    // Stable: equal ranks keep root and path order, so paging is deterministic.
    ranked.sort_by_key(|(rank, _)| std::cmp::Reverse(*rank));
    let module_matches: Vec<_> = ranked.into_iter().map(|(_, export)| export).collect();
    let mut module_rows = Vec::new();
    for export in module_matches.iter().skip(request.offset).take(request.limit) {
        let mut row = emath_artifact::JsonWriter::object();
        row.string("name", &export.name);
        row.string("kind", &export.kind);
        row.string("module", &export.module);
        row.string("shelf", export.shelf);
        row.string("signature", &export.signature);
        row.string("summary", &export.summary);
        row.string("file", &export.file.display().to_string());
        row.int("line", export.line as u64);
        if !request.json {
            println!(
                "use {}: {}\n  {}{}:{}",
                export.module,
                export.signature,
                if export.summary.is_empty() {
                    String::new()
                } else {
                    format!("{}\n  ", export.summary)
                },
                export.file.display(),
                export.line
            );
        }
        module_rows.push(row.finish());
    }
    let module_next = request.offset.saturating_add(module_rows.len());
    let mut commands = Vec::new();
    for name in [
        "api", "check", "run", "step", "inspect", "test", "build",
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
            "emath function Answer:\n    inputs:\n\n    outputs:\n        result: Int\n\n    definitions:\n        result = 2 + 1\n",
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
            "emath.constructor.v1: execution, fulfillment, representation, payload, evidence, remaining",
        );
        out.string(
            "continuation",
            "constructor-layer checkpoint; `emath step` resumes the saved continuation",
        );
        out.string("parameter_values", "--set supports Bool, Int, Rat, Float64, and sequences ([1, 2] or [1/2, 3/4]); --set-file reads a JSON object of those values");
        out.objects("commands", &commands);
        out.string("search", &request.search);
        out.int("offset", request.offset as u64);
        out.int("matching_features", matches.len() as u64);
        out.bool("truncated", next < matches.len());
        if next < matches.len() {
            out.int("next_offset", next as u64);
        }
        out.objects("features", &features);
        out.int("matching_module_exports", module_matches.len() as u64);
        out.bool("module_exports_truncated", module_next < module_matches.len());
        out.objects("module_exports", &module_rows);
        println!("{}", out.finish());
    } else {
        println!(
            "{} matching features; showing {}..{}",
            matches.len(),
            request.offset.min(matches.len()),
            next.min(matches.len())
        );
        println!(
            "{} matching module exports; showing {}..{}",
            module_matches.len(),
            request.offset.min(module_matches.len()),
            module_next.min(module_matches.len())
        );
        println!("write program.emath -> emath check program.emath -> emath run program.emath");
        println!("Use --json for command arguments, result contracts and the starter source.");
        if next < matches.len() || module_next < module_matches.len() {
            println!(
                "More results: add --offset {} with the same --search.",
                next.max(module_next)
            );
        }
    }
    crate::EXIT_OK
}

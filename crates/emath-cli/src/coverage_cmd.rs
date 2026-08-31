//! `emath coverage`: language completeness coverage ledger (bead
//! emath-r3-coverage-ledger-3ism, 05 section 6).
//!
//! The ledger answers "how much of math is missing?" as a generated number,
//! never a guess. Axis 1 is MSC 2020 top-level domains grouped into
//! super-domains; axis 2 is six facets per domain (types, operators, goals,
//! notation, worlds, evidence). Support levels are ordered:
//! `none < planned < contract < reference-impl < provider-backed < certified`.
//!
//! Levels are asserted by artifacts, not authors: a `reference-impl`+ facet
//! must link an artifact (a runnable example or an evidence id), or
//! generation fails with `E-COV-UNEVIDENCED-LEVEL`. Links below
//! `reference-impl` are provenance, not evidence; they never satisfy the gate.
//!
//! The seed dataset imports the Phase 3a MSC matrix (02 B01-B46) through the
//! rating vocabulary mapping: FULL -> reference-impl, SYNTAX-ONLY -> contract,
//! MISSING -> none, PARTIAL -> per-facet split (the seed stores the split
//! directly as facet ratings, never a wholesale PARTIAL). An unknown rating
//! word fails generation with `E-COV-BAD-RATING`.
//!
//! Output is canonical JSON (`emath.coverage-ledger v1`), byte-identical on
//! rerun for identical inputs. `--check <file>` regenerates and compares
//! byte-exactly, refusing drift with `E-COV-DRIFT`. Std-only and
//! deterministic: no clocks, no environment reads, seed-declaration order.

#![forbid(unsafe_code)]

use crate::coverage_seed::{self, DomainSeed};
use crate::CliExit;
use std::path::Path;

/// Ordered support levels. Index order is the partial order.
pub const SUPPORT_LEVELS: [&str; 6] = [
    "none",
    "planned",
    "contract",
    "reference-impl",
    "provider-backed",
    "certified",
];

/// The six facets asserted per domain.
pub const FACETS: [&str; 6] = ["types", "operators", "goals", "notation", "worlds", "evidence"];

/// Minimum level (index) that counts toward coverage.
pub const COVERAGE_THRESHOLD: usize = 3; // reference-impl

/// E-COV code for a `reference-impl`+ claim with no linked artifact.
pub const E_UNEVIDENCED: &str = "E-COV-UNEVIDENCED-LEVEL";
/// E-COV code for `--check` drift between disk and regenerated output.
pub const E_DRIFT: &str = "E-COV-DRIFT";
/// E-COV code for a rating word outside the closed vocabulary.
pub const E_BAD_RATING: &str = "E-COV-BAD-RATING";
/// E-COV code for a ledger artifact link that does not exist under the root.
pub const E_MISSING_ARTIFACT: &str = "E-COV-MISSING-ARTIFACT";
/// E-COV code for a PACKAGE_CATALOG row no seed domain claims.
pub const E_PACKAGE_UNCLAIMED: &str = "E-COV-PACKAGE-UNCLAIMED";
/// E-COV code for a seed-claimed package absent from PACKAGE_CATALOG.
pub const E_PACKAGE_UNKNOWN: &str = "E-COV-PACKAGE-UNKNOWN";

/// Map one Phase 3a matrix rating to its support level. PARTIAL has no
/// wholesale mapping; the seed splits it per facet before this runs.
#[must_use]
pub fn rating_to_level(rating: &str) -> Option<usize> {
    match rating {
        "FULL" => Some(3),        // reference-impl
        "SYNTAX-ONLY" => Some(2), // contract
        "MISSING" => Some(0),     // none
        _ => None,
    }
}

/// Resolve a seed row's facet ratings into level indices, refusing rating
/// words outside the vocabulary.
pub fn resolve_levels(seed: &DomainSeed) -> Result<[usize; 6], String> {
    let mut levels = [0usize; 6];
    for (index, rating) in seed.ratings.iter().enumerate() {
        let Some(level) = rating_to_level(rating) else {
            return Err(format!(
                "{E_BAD_RATING}: domain {} facet {} has rating `{rating}`; \
                 expected FULL, SYNTAX-ONLY, MISSING, or a per-facet PARTIAL split",
                seed.msc, FACETS[index],
            ));
        };
        levels[index] = level;
    }
    Ok(levels)
}

/// CI gate: every cited artifact must exist under `root` (the repo root when
/// the CLI runs). A ledger entry citing a nonexistent example fails here.
pub fn verify_artifacts(root: &Path) -> Result<(), String> {
    for seed in coverage_seed::SEED.iter() {
        for artifact in seed.artifacts.iter().flatten() {
            if !root.join(artifact).exists() {
                return Err(format!(
                    "{E_MISSING_ARTIFACT}: domain {} cites missing artifact {artifact}",
                    seed.msc
                ));
            }
        }
    }
    Ok(())
}

/// CI gate: every `PACKAGE_CATALOG.md` row must be claimed by exactly one
/// seed domain, and every seed-claimed package must exist in the catalog.
/// `catalog_path` is the PACKAGE_CATALOG.md file; missing file refuses.
pub fn verify_packages(catalog_path: &Path) -> Result<(), String> {
    let catalog = std::fs::read_to_string(catalog_path).map_err(|error| {
        format!(
            "E-COV-NO-CATALOG: cannot read {}: {error}",
            catalog_path.display()
        )
    })?;
    let mut catalog_packages: Vec<&str> = Vec::new();
    for line in catalog.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("| `") && !trimmed.starts_with("| [`") {
            continue;
        }
        let name = trimmed
            .trim_start_matches("| ")
            .trim_start_matches(['[', '`']);
        let Some(name) = name.split('`').next() else {
            continue;
        };
        if !name.is_empty() && name != "Package" {
            catalog_packages.push(name);
        }
    }
    let claimed: Vec<&str> = coverage_seed::SEED
        .iter()
        .flat_map(|seed| seed.packages.iter().copied())
        .collect();
    for package in &catalog_packages {
        if !claimed.contains(package) {
            return Err(format!(
                "{E_PACKAGE_UNCLAIMED}: PACKAGE_CATALOG row `{package}` has no ledger domain entry"
            ));
        }
    }
    for package in &claimed {
        if !catalog_packages.contains(package) {
            return Err(format!(
                "{E_PACKAGE_UNKNOWN}: ledger claims `{package}` but PACKAGE_CATALOG has no such row"
            ));
        }
    }
    Ok(())
}

/// Canonical JSON ledger document. Byte-identical across runs.
pub fn ledger_json() -> Result<String, String> {
    let mut domain_docs: Vec<String> = Vec::new();
    let mut covered_facets = 0u64;
    for seed in coverage_seed::SEED.iter() {
        let levels = resolve_levels(seed)?;
        // Evidence gate: reference-impl+ facets must pin an artifact.
        for (facet_index, level) in levels.iter().enumerate() {
            if *level >= COVERAGE_THRESHOLD && seed.artifacts[facet_index].is_none() {
                return Err(format!(
                    "{E_UNEVIDENCED}: domain {} facet {} claims level {} without a linked artifact",
                    seed.msc, FACETS[facet_index], SUPPORT_LEVELS[*level],
                ));
            }
        }
        let mut facet_docs: Vec<String> = Vec::new();
        let mut covered = 0u64;
        for (facet_index, facet) in FACETS.iter().enumerate() {
            let level = levels[facet_index];
            if level >= COVERAGE_THRESHOLD {
                covered += 1;
                covered_facets += 1;
            }
            let mut facet_obj = emath_artifact::JsonWriter::object();
            facet_obj.string("facet", facet);
            facet_obj.string("rating", seed.ratings[facet_index]);
            facet_obj.string("level", SUPPORT_LEVELS[level]);
            if let Some(artifact) = seed.artifacts[facet_index] {
                facet_obj.string("artifact", artifact);
            }
            facet_docs.push(facet_obj.finish());
        }
        let mut domain_obj = emath_artifact::JsonWriter::object();
        domain_obj.string("msc", seed.msc);
        domain_obj.string("super_domain", seed.super_domain);
        domain_obj.string("label", seed.label);
        domain_obj.int(
            "coverage_pct",
            covered * 100 / FACETS.len() as u64,
        );
        domain_obj.objects("facets", &facet_docs);
        let packages: Vec<String> = seed.packages.iter().map(|package| package.to_string()).collect();
        domain_obj.strings("packages", &packages);
        domain_docs.push(domain_obj.finish());
    }

    let total_domains = coverage_seed::SEED.len() as u64;
    let total_facets = total_domains * FACETS.len() as u64;
    let overall = covered_facets * 100 / total_facets;

    let mut missing_by_facet: Vec<String> = Vec::new();
    for seed in coverage_seed::SEED.iter() {
        let levels = resolve_levels(seed)?;
        for (facet_index, facet) in FACETS.iter().enumerate() {
            if levels[facet_index] < COVERAGE_THRESHOLD {
                let mut entry = emath_artifact::JsonWriter::object();
                entry.string("msc", seed.msc);
                entry.string("facet", facet);
                missing_by_facet.push(entry.finish());
            }
        }
    }

    let mut root = emath_artifact::JsonWriter::object();
    root.string("schema", "emath.coverage-ledger");
    root.string("schema_version", "v1");
    root.string(
        "rating_vocabulary",
        "FULL->reference-impl, SYNTAX-ONLY->contract, MISSING->none, PARTIAL->per-facet split",
    );
    root.string("support_levels", &SUPPORT_LEVELS.join("<"));
    root.string(
        "seed_provenance",
        "Phase 3a MSC matrix (02 B01-B46) rollup: 5 FULL, 16 PARTIAL, 14 SYNTAX-ONLY, 22 MISSING across 57 sub-areas; imported at super-domain granularity",
    );
    root.int("domains", total_domains);
    root.int("facets_total", total_facets);
    root.int("facets_covered", covered_facets);
    root.int("coverage_pct", overall);
    root.int("missing_math_pct", 100 - overall);
    root.objects("missing_by_facet", &missing_by_facet);
    root.objects("domains", &domain_docs);
    Ok(root.finish())
}

/// `--check <ledger-file>`: regenerate and compare byte-exactly. Drift or an
/// unreadable ledger refuses.
pub fn check_against_disk(generated: &str, ledger_path: &Path) -> Result<bool, String> {
    let stored = std::fs::read_to_string(ledger_path).map_err(|error| {
        format!(
            "E-COV-NO-LEDGER: cannot read {}: {error}",
            ledger_path.display()
        )
    })?;
    Ok(stored == generated)
}

/// `emath coverage [--emit json] [--check <ledger-file>]`.
///
/// - `--emit json`: canonical JSON on stdout.
/// - default: rendered Markdown dashboard on stdout.
/// - `--check <file>`: regenerate and compare byte-exactly; refuse drift.
pub fn coverage_cmd(rest: &[String]) -> CliExit {
    let mut saw_emit = false;
    let mut saw_check = false;
    let mut emit_json = false;
    let mut check_path: Option<&str> = None;
    for arg in rest {
        match arg.as_str() {
            "--emit" if !saw_emit && !saw_check => saw_emit = true,
            "json" if saw_emit && !emit_json => emit_json = true,
            "--check" if !saw_check && !saw_emit => saw_check = true,
            path if saw_check && check_path.is_none() && !path.starts_with('-') => {
                check_path = Some(path);
            }
            _ => return usage_coverage("coverage [--emit json] [--check <ledger-file>]"),
        }
    }
    if saw_check && check_path.is_none() {
        return usage_coverage("--check requires a ledger file path");
    }
    if saw_emit && !emit_json {
        return usage_coverage("--emit requires `json`");
    }
    if emit_json || saw_check {
        if let Err(message) = verify_artifacts(Path::new(".")) {
            eprintln!("error: {message}");
            return CliExit::Refused;
        }
        if let Err(message) = verify_packages(Path::new("language/stdlib/PACKAGE_CATALOG.md")) {
            eprintln!("error: {message}");
            return CliExit::Refused;
        }
    }

    match (emit_json, check_path) {
        (true, None) => match ledger_json() {
            Ok(document) => {
                print!("{document}");
                CliExit::Ok
            }
            Err(message) => {
                eprintln!("error: {message}");
                CliExit::Refused
            }
        },
        (false, Some(path)) => {
            let generated = match ledger_json() {
                Ok(document) => document,
                Err(message) => {
                    eprintln!("error: {message}");
                    return CliExit::Refused;
                }
            };
            match check_against_disk(&generated, Path::new(path)) {
                Ok(true) => {
                    println!("coverage ledger up to date: {path}");
                    CliExit::Ok
                }
                Ok(false) => {
                    eprintln!("error: {E_DRIFT}: {path} does not match regenerated ledger");
                    CliExit::Refused
                }
                Err(message) => {
                    eprintln!("error: {message}");
                    CliExit::Refused
                }
            }
        }
        (false, None) => {
            print_markdown_table();
            CliExit::Ok
        }
        (true, Some(_)) => usage_coverage("--emit json cannot be combined with --check"),
    }
}

fn usage_coverage(message: &str) -> CliExit {
    eprintln!("error: {message}");
    crate::usage("coverage [--emit json] [--check <ledger-file>]")
}

/// Rendered public dashboard: one row per super-domain.
fn print_markdown_table() {
    println!("# emath coverage ledger (v1)");
    println!();
    println!("| super-domain | msc | facets covered | coverage % |");
    println!("|---|---|---|---:|");
    for seed in coverage_seed::SEED.iter() {
        let Ok(levels) = resolve_levels(seed) else {
            continue;
        };
        let covered = levels
            .iter()
            .filter(|level| **level >= COVERAGE_THRESHOLD)
            .count();
        let pct = covered * 100 / FACETS.len();
        println!(
            "| {} | {} | {covered}/{} | {pct} |",
            seed.super_domain,
            seed.msc,
            FACETS.len()
        );
    }
}

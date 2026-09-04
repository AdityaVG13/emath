//! The `emath migrate` workspace-upgrade command.

use super::*;

/// `migrate <file.emath> [--fix] [--check] [--receipt <path>] | migrate
/// --list-rules` (05 §5). Lossless rewrites only, receipt-driven.
///
/// The canonical-format rule (E-MIG-RULE-001) is the registered rule
/// wired here: the lossless formatter rewrite, verified by re-lowering
/// both sides via the migrate contract engine. The file is rewritten
/// ONLY under `--fix` and only when identity verified; the receipt is
/// written to `--receipt` (default: beside the source). `--check`
/// never rewrites; exit 1 means a rule would fire (or the source
/// refuses). Determinism: same input = byte-identical receipt.
pub(crate) fn migrate_cmd(
    file: &Path,
    fix: bool,
    check_only: bool,
    receipt: Option<&Path>,
    list_rules: bool,
) -> CliExit {
    if list_rules {
        for rule in emath_sema::migrate::registered_rules() {
            println!("{}\t{}\t{}", rule.id, rule.kind.as_str(), rule.description);
        }
        return EXIT_OK;
    }
    let Ok(source) = std::fs::read_to_string(file) else {
        eprintln!("error: cannot read {}", file.display());
        return EXIT_USAGE;
    };
    // The registered rewrite: canonical-format respell (lossless
    // formatter). The verify engine owns admission + identity checking.
    let limits = emath_core::limits::Limits::default();
    let lossless = emath_syntax::parse_lossless(&source, emath_core::FileId(0), &limits);
    let rewritten = emath_syntax::format_lossless(&lossless);
    let outcome = emath_sema::migrate::migrate_verified_rewrite(
        &file.display().to_string(),
        &source,
        &rewritten,
        emath_sema::migrate::RULE_CANONICAL_FORMAT.id,
    );
    let receipt_path = receipt
        .map(Path::to_path_buf)
        .unwrap_or_else(|| file.with_extension("migrate.json"));
    if std::fs::write(&receipt_path, outcome.receipt.to_canonical_json()).is_err() {
        eprintln!("error: cannot write receipt {}", receipt_path.display());
        return EXIT_USAGE;
    }
    if check_only {
        // Never rewrites: report whether a rule would fire.
        if outcome.receipt.refusals.is_empty() && outcome.receipt.rules_applied.is_empty() {
            println!("migrate: {}: canonical (no rules to apply)", file.display());
            return EXIT_OK;
        }
        for refusal in &outcome.receipt.refusals {
            eprintln!(
                "migrate: {}: {} {}",
                file.display(),
                refusal.code,
                refusal.reason
            );
        }
        if !outcome.receipt.rules_applied.is_empty() {
            for rule in &outcome.receipt.rules_applied {
                eprintln!(
                    "migrate: {}: {} ({}) would apply",
                    file.display(),
                    rule.rule,
                    rule.kind.as_str()
                );
            }
            return EXIT_REFUSED;
        }
        return EXIT_REFUSED;
    }
    for refusal in &outcome.receipt.refusals {
        eprintln!(
            "migrate: {}: {} {}",
            file.display(),
            refusal.code,
            refusal.reason
        );
    }
    match (fix, outcome.rewritten_source.as_deref()) {
        (true, Some(rewritten)) => {
            if std::fs::write(file, rewritten).is_err() {
                eprintln!("error: cannot rewrite {}", file.display());
                return EXIT_USAGE;
            }
            println!(
                "migrate: {}: rewritten (receipt: {})",
                file.display(),
                receipt_path.display()
            );
            EXIT_OK
        }
        (true, None) => EXIT_REFUSED,
        (false, Some(_)) => {
            // No --fix: the rewrite is NOT emitted; report only.
            eprintln!(
                "migrate: {}: rule would fire (pass --fix to apply); receipt: {}",
                file.display(),
                receipt_path.display()
            );
            EXIT_REFUSED
        }
        (false, None) if outcome.receipt.refusals.is_empty() => {
            println!(
                "migrate: {}: canonical (no rules to apply); receipt: {}",
                file.display(),
                receipt_path.display()
            );
            EXIT_OK
        }
        (false, None) => EXIT_REFUSED,
    }
}

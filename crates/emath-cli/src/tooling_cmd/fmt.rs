//! The `emath fmt` / `emath fmt-value` formatting commands.

use super::*;

/// `fmt <file>`: canonical-form check via the lossless formatter;
/// canonical only on byte-for-byte round-trip, else refusal + diff.
/// `emath fmt --value <literal>`: sig-fig rounding + unit-preserving
/// display (04 §1.6+1.7).
///
/// Deterministic; units resolve from the std seed table; bare value
/// output rounds to the input literal's own significant-figure count
/// ("rounds output to minimum input sf"). Unit-preserving display
/// (§1.7) changes presentation only: the converted value is re-reported
/// as-is (`90 s` → `1.5 min`), and sf rounding applies only with an
/// explicit `--sf`. Incompatible format unit is refused (`E-UNIT-FMT`);
/// `--from` without `--format` is refused (`E-UNIT-104` path for unknown
/// units).
pub(crate) fn fmt_value_cmd(
    value_raw: &str,
    sf: Option<u32>,
    from: Option<&str>,
    format: Option<&str>,
) -> CliExit {
    let Ok(value) = value_raw.parse::<f64>() else {
        eprintln!("error: --value must be a decimal literal, found `{value_raw}`");
        return EXIT_USAGE;
    };
    let table = emath_core::seed_table();
    let literal_sf = sf.or_else(|| emath_core::count_sig_figs(value_raw));
    let spec = match format {
        Some(text) => match emath_core::FormatSpec::parse(text) {
            Ok(spec) => Some(spec),
            Err(err) => {
                eprintln!("error: {}: {}", err.code, err.message);
                return EXIT_REFUSED;
            }
        },
        None => None,
    };
    match spec {
        None => {
            if from.is_some() {
                // `--from` only matters for a display format; refusing
                // beats silently ignoring it.
                eprintln!("error: --from requires --format");
                return EXIT_USAGE;
            }
            let rounded = match literal_sf {
                Some(n) if n > 0 => emath_core::round_to_sig_figs(value, n),
                _ => value,
            };
            println!("{rounded}");
            EXIT_OK
        }
        Some(parsed) => {
            let unit = match (&parsed, from) {
                (emath_core::FormatSpec::PreferredUnit { .. }, None) => {
                    eprintln!(
                        "error: {}: preferred_unit requires --from <unit>",
                        emath_core::E_UNIT_FMT
                    );
                    return EXIT_REFUSED;
                }
                (_, Some(name)) => match table.resolve(name) {
                    Ok(unit) => unit,
                    Err(err) => {
                        eprintln!("error: {}: {}", err.code, err.message);
                        return EXIT_REFUSED;
                    }
                },
                (_, None) => emath_core::UnitSpec::new("1", [0; 7], 1.0, 0.0),
            };
            let quantity = emath_core::Quantity {
                value,
                unit,
                kind: emath_core::QuantityKind::Absolute,
            };
            let formatted = emath_core::FormattedQuantity {
                quantity,
                format: parsed,
            };
            match formatted.display(&table, sf) {
                Ok(text) => {
                    println!("{text}");
                    EXIT_OK
                }
                Err(err) => {
                    eprintln!("error: {}: {}", err.code, err.message);
                    EXIT_REFUSED
                }
            }
        }
    }
}

pub(crate) fn fmt_cmd(file: &Path) -> CliExit {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(file) else {
        eprintln!("error: cannot read {}", file.display());
        return EXIT_USAGE;
    };
    let result = session.check(package.file);
    print_diagnostics(&result.diagnostics);
    if result.diagnostics.has_errors() {
        return EXIT_REFUSED;
    }
    let limits = emath_core::limits::Limits::default();
    let lossless = emath_syntax::parse_lossless(&package.text, package.file, &limits);
    let canonical = emath_syntax::format(&lossless.tree, &lossless.comments);
    if canonical == package.text {
        println!(
            "fmt: {}: canonical form (lossless round-trip)",
            file.display()
        );
        EXIT_OK
    } else {
        eprintln!(
            "fmt: {}: NOT canonical; expected lossless formatter output",
            file.display()
        );
        for (line_no, (expected, actual)) in canonical
            .lines()
            .zip(package.text.lines())
            .enumerate()
            .filter(|(_, (expected, actual))| expected != actual)
            .take(10)
        {
            eprintln!(
                "  line {}: expected `{expected}`, found `{actual}`",
                line_no + 1
            );
        }
        EXIT_REFUSED
    }
}

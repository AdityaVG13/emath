#![forbid(unsafe_code)]
//! Negative parser witnesses: constructs the Phase 1 strict subset does not
//! implement must be refused with a stable diagnostic instead of being
//! parsed into lossy trees (fn types as Path(["fn"]), type aliases with
//! the RHS dropped, generic extern operator parameters discarded, broken
//! section argument lists stored as args: None).

use emath_syntax::parse_str;
use emath_test_harness::Probe;

fn error_codes(text: &str) -> Vec<String> {
    let (_, diagnostics) = parse_str(text);
    diagnostics
        .errors()
        .map(|error| error.code.to_string())
        .collect()
}

#[test]
fn probe() {
    let mut p = Probe::new(
        "Phase 1 refuses fn types, type aliases, generic extern operators, and broken args; the baseline parses clean",
    );
    for (name, source, code) in [
        (
            "fn-type",
            include_str!("fixtures/fn_type.emath"),
            Some("E-TYPE-110"),
        ),
        (
            "type-alias",
            include_str!("fixtures/type_alias.emath"),
            Some("E-TYPE-111"),
        ),
        (
            "generic-extern",
            include_str!("fixtures/extern_op.emath"),
            Some("E-TYPE-112"),
        ),
        (
            "broken-args",
            include_str!("fixtures/broken_args.emath"),
            Some("E-SYN-101"),
        ),
        ("baseline", include_str!("fixtures/baseline.emath"), None),
    ] {
        p.case(name, |p| {
            let found = error_codes(source);
            match code {
                Some(want) => {
                    p.demand(
                        want,
                        found.iter().any(|got| got == want),
                        format!("must refuse with {want}, got {found:?}"),
                    );
                }
                None => {
                    p.eq("errors", found, Vec::<String>::new());
                }
            }
        });
    }
    p.finish();
}

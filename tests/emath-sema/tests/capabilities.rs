//! ELP experimental-lane gates: `@capabilities` / `@experimental` item
//! attributes and their typed refusals (see `elps/README.md`).
//!
//! Intent: experimental syntax must never compile silently in a stable
//! package — every gate below proves either the refusal (E-PKG-064 /
//! E-SYN-117 / E-SYN-118 / E-PKG-065) or the declared-capability admit.

use emath_test_harness::{Probe, Source, boot};

fn function_source(prefix: &str) -> String {
    format!(
        "{prefix}emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n"
    )
}

#[test]
fn experimental_lane_gates() {
    boot();
    let mut p = Probe::new("experimental syntax never compiles silently in a stable package");
    p.case("refusals", |p| {
        Source::from_str("no-capability", function_source("@experimental\n"))
            .must_refuse(&mut *p, &["E-PKG-064"]);
        Source::from_str("unknown-attribute", function_source("@bogus\n"))
            .must_refuse(&mut *p, &["E-SYN-118"]);
        Source::from_str("unknown-key", function_source("@capabilities(teleportation)\n"))
            .must_refuse(&mut *p, &["E-PKG-065"]);
        Source::from_str("experimental-args", function_source("@experimental(deep)\n"))
            .must_refuse(&mut *p, &["E-SYN-117"]);
        Source::from_str("malformed-args", function_source("@capabilities(x = 1)\n"))
            .must_refuse(&mut *p, &["E-SYN-117"]);
    });
    p.case("admits", |p| {
        Source::from_str(
            "declared",
            function_source("@capabilities(experimental-syntax)\n@experimental\n"),
        )
        .must_admit(&mut *p);
        Source::from_str(
            "quoted-key",
            function_source("@capabilities(\"experimental-syntax\")\n@experimental\n"),
        )
        .must_admit(&mut *p);
        Source::from_str(
            "file-scoped",
            format!(
                "{}{}",
                function_source("@capabilities(experimental-syntax)\n"),
                function_source("@experimental\n").replace("function P:", "function Q:"),
            ),
        )
        .must_admit(&mut *p);
    });
    p.finish();
}

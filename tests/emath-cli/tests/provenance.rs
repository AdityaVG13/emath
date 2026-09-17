//! `provenance_explanation` on the constructor surface.
//!
//! The `provenance:` meta-section grammar was pruned with the recipe
//! lane (it is not a constructor section), so a file carrying it refuses
//! at admission. What stays live: the explanation renders the binding
//! provenance the constructor admission records for a valid file.

use emath_cli::{EXIT_REFUSED, provenance_explanation};
use emath_test_harness::{Probe, boot};

#[test]
fn probe() {
    boot();
    let mut p = Probe::new(
        "provenance explanation renders constructor bindings; the pruned provenance section refuses",
    );
    let valid = std::env::temp_dir().join(format!("emath-prov-ok-{}", std::process::id()));
    std::fs::write(
        &valid,
        "emath function Calibration:\n    inputs:\n        value: Float64\n    outputs:\n        result: Float64\n    definitions:\n        result = value\n",
    )
    .expect("fixture");
    match provenance_explanation(&valid, false) {
        Ok(text) => {
            p.contains("renders", &text, "provenance");
        }
        Err(_) => {
            p.fail("renders", "a constructor-valid file must explain");
        }
    }

    let sectioned = std::env::temp_dir().join(format!("emath-prov-bad-{}", std::process::id()));
    std::fs::write(
        &sectioned,
        "emath function Calibration:\n    inputs:\n        value: Float64\n        missing_source: Float64\n    definitions:\n        result = value\n    provenance:\n        value:\n            kind: \"Assumed\"\n            reason: \"calibration fixture\"\n        missing_source:\n            kind: \"Unstated\"\n",
    )
    .expect("fixture");
    p.eq(
        "pruned-section",
        provenance_explanation(&sectioned, false).err(),
        Some(EXIT_REFUSED),
    );
    p.finish();
}

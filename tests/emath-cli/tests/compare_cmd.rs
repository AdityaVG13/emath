//! `emath compare` end-to-end over `tests/fixtures/compare/worlds.emath`:
//! one definition, three carrier worlds, every answer reported.
//!
//! - `(1 + x) - 1 - x` at x = 0.1 is exactly 0 in the exact worlds and a
//!   nonzero binary64 residue in the float64 world: `disagree`, with the
//!   gap carried.
//! - `x + 0.2 == 0.3` mixes an exact input with a float literal: the
//!   authored reference takes the binary64 lane (false), the exact world
//!   reads the literals exactly (true), float64 is false.
//! - `1/n + 1/n + 1/n == 3/n` at n = 10 holds exactly; in float64 the
//!   integer quotients round and the sum misses: `disagree`. `1/n` alone
//!   is a binary64 in the float64 world, 2^-54-ish away from 1/10.
//! - Integer arithmetic is the same in every world: `agree`.
//! - `x.denom` has no meaning for a binary64: the float64 world refuses
//!   by name while the exact worlds answer 10: `partial`.

mod common;

use std::path::PathBuf;
use std::process::Command;

use common::emath_bin;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn compare(function: &str, set: &str) -> String {
    let fixture = workspace().join("tests/fixtures/compare/worlds.emath");
    let out = Command::new(emath_bin())
        .arg("compare")
        .arg(&fixture)
        .args(["--function", function, "--set", set, "--json"])
        .env("EMATH_ROOT", workspace())
        .output()
        .expect("run emath compare");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The JSON row of one world, as compact text.
fn world_row(json: &str, world: &str) -> String {
    let compact: String = json.chars().filter(|c| !c.is_whitespace()).collect();
    let key = format!("\"world\":\"{world}\"");
    let at = compact
        .find(&key)
        .unwrap_or_else(|| panic!("no {world} row in {json}"));
    let end = compact[at..].find('}').map_or(compact.len(), |i| at + i);
    compact[at..end].to_string()
}

fn verdict(json: &str) -> String {
    let compact: String = json.chars().filter(|c| !c.is_whitespace()).collect();
    let at = compact.find("\"verdict\":\"").expect("verdict") + 11;
    compact[at..].split('"').next().unwrap().to_string()
}

fn claim_id(row: &str) -> String {
    let at = row.find("\"claim\":{\"id\":\"").expect("claim id") + 15;
    row[at..].split('"').next().unwrap().to_string()
}

#[test]
fn compare_cmd() {
    let out = compare("cancellation", "x=0.1");
    assert_eq!(verdict(&out), "disagree", "{out}");
    // Every answer carries an unchecked claim naming the inputs as that
    // world read them; its id is reproducible and distinct per world,
    // even where two worlds return the same value.
    let reference = world_row(&out, "reference");
    assert!(
        reference.contains("\"statement\":\"cancellation(x=1/10)=0/1inworldreference\""),
        "{out}"
    );
    assert!(reference.contains("\"status\":\"unchecked\""), "{out}");
    assert!(
        world_row(&out, "float64").contains("\"statement\":\"cancellation(x=0.1)="),
        "{out}"
    );
    let again = compare("cancellation", "x=0.1");
    assert_eq!(
        claim_id(&reference),
        claim_id(&world_row(&again, "reference")),
        "{out}"
    );
    assert_ne!(
        claim_id(&reference),
        claim_id(&world_row(&out, "exact")),
        "{out}"
    );
    assert!(
        world_row(&out, "reference").contains("\"payload\":\"0/1\""),
        "{out}"
    );
    assert!(
        world_row(&out, "exact").contains("\"payload\":\"0/1\""),
        "{out}"
    );
    let float = world_row(&out, "float64");
    assert!(
        float.contains("\"representation\":\"rounded_scalar\""),
        "{out}"
    );
    assert!(float.contains("\"gap\":8.326672684688674e-17"), "{out}");

    let out = compare("crosses", "x=0.1");
    assert_eq!(verdict(&out), "disagree", "{out}");
    assert!(
        world_row(&out, "reference").contains("\"payload\":\"false\""),
        "{out}"
    );
    assert!(
        world_row(&out, "exact").contains("\"payload\":\"true\""),
        "{out}"
    );
    assert!(
        world_row(&out, "float64").contains("\"payload\":\"false\""),
        "{out}"
    );

    let out = compare("thirds_add_up", "n=10");
    assert_eq!(verdict(&out), "disagree", "{out}");
    assert!(
        world_row(&out, "reference").contains("\"payload\":\"true\""),
        "{out}"
    );
    assert!(
        world_row(&out, "exact").contains("\"payload\":\"true\""),
        "{out}"
    );
    assert!(
        world_row(&out, "float64").contains("\"payload\":\"false\""),
        "{out}"
    );

    let out = compare("tenth", "n=10");
    assert_eq!(verdict(&out), "disagree", "{out}");
    assert!(
        world_row(&out, "exact").contains("\"payload\":\"1/10\""),
        "{out}"
    );
    let float = world_row(&out, "float64");
    assert!(float.contains("\"payload\":\"0.1\""), "{out}");
    assert!(float.contains("\"gap\":5.551115123125783e-18"), "{out}");

    let out = compare("square_plus_one", "n=7");
    assert_eq!(verdict(&out), "agree", "{out}");
    for world in ["reference", "exact", "float64"] {
        assert!(
            world_row(&out, world).contains("\"payload\":\"50\""),
            "{out}"
        );
    }

    let out = compare("denominator", "x=0.1");
    assert_eq!(verdict(&out), "partial", "{out}");
    assert!(
        world_row(&out, "reference").contains("\"payload\":\"10\""),
        "{out}"
    );
    assert!(
        world_row(&out, "exact").contains("\"payload\":\"10\""),
        "{out}"
    );
    let float = world_row(&out, "float64");
    assert!(float.contains("\"execution\":\"refused\""), "{out}");
    assert!(float.contains("\"code\":\"type\""), "{out}");
}

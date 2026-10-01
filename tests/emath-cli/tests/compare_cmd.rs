//! `emath compare` end-to-end over `tests/fixtures/compare/worlds.emath`:
//! one definition, every carrier world, every answer reported.
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
//! - Four RK4 steps of y' = y: the exact worlds agree on a rational; the
//!   float64 world passes RK4's exact order-condition guard (the tableau
//!   is a program constant) and integrates in binary64: `disagree`.
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
    compare_in(function, set, &[])
}

fn compare_in(function: &str, set: &str, worlds: &[&str]) -> String {
    let mut args = vec!["--set", set];
    args.extend(worlds.iter().flat_map(|world| ["--world", world]));
    compare_args(function, &args)
}

fn compare_args(function: &str, args: &[&str]) -> String {
    let fixture = workspace().join("tests/fixtures/compare/worlds.emath");
    let out = Command::new(emath_bin())
        .arg("compare")
        .arg(&fixture)
        .args(["--function", function, "--json"])
        .args(args)
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
    // In binary32 the rounded sum lands on the rounded 0.3 (and in
    // bfloat16 too): the answer depends on the format, not just on
    // whether it rounds.
    assert!(
        world_row(&out, "float32").contains("\"payload\":\"true\""),
        "{out}"
    );
    assert!(
        world_row(&out, "bfloat16").contains("\"payload\":\"true\""),
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
    assert!(
        world_row(&out, "float32").contains("\"payload\":\"0.10000000149011612\""),
        "{out}"
    );
    assert!(
        world_row(&out, "bfloat16").contains("\"payload\":\"0.10009765625\""),
        "{out}"
    );

    // Inputs 1e-25 above and below the binary32 midpoint 1 + 2^-24. Both
    // read in binary64 as that midpoint exactly (ties-to-even would give
    // 1 for both); rounding from the caller's exact decimal splits them.
    let out = compare("keep", "x=1.0000000596046447753906251");
    assert!(
        world_row(&out, "float32").contains("\"payload\":\"1.0000001192092896\""),
        "{out}"
    );
    let out = compare("keep", "x=1.0000000596046447753906249");
    assert!(
        world_row(&out, "float32").contains("\"payload\":\"1\""),
        "{out}"
    );

    let out = compare("square_plus_one", "n=7");
    assert_eq!(verdict(&out), "agree", "{out}");
    for world in [
        "reference",
        "exact",
        "float64",
        "float32",
        "bfloat16",
        "int64",
    ] {
        assert!(
            world_row(&out, world).contains("\"payload\":\"50\""),
            "{out}"
        );
    }
    // 3037000500^2 passes 2^63 - 1: the exact worlds grow, the int64
    // world refuses by name.
    let out = compare("square_plus_one", "n=3037000500");
    assert_eq!(verdict(&out), "partial", "{out}");
    assert!(
        world_row(&out, "reference").contains("\"payload\":\"9223372037000250001\""),
        "{out}"
    );
    let machine = world_row(&out, "int64");
    assert!(machine.contains("\"execution\":\"refused\""), "{out}");
    assert!(machine.contains("\"code\":\"machine_overflow\""), "{out}");

    // RK4 guards its tableau with exact order conditions. In float64 the
    // tableau is a program constant, so the guard holds and the stepping
    // itself runs in binary64: an answer near the exact one, not a
    // refusal.
    let out = compare("grow", "steps=4");
    assert_eq!(verdict(&out), "disagree", "{out}");
    assert!(
        world_row(&out, "exact").contains("\"payload\":\"3873359651615041/1424967069597696\""),
        "{out}"
    );
    let float = world_row(&out, "float64");
    assert!(float.contains("\"execution\":\"returned\""), "{out}");
    assert!(
        float.contains("\"representation\":\"rounded_scalar\""),
        "{out}"
    );
    assert!(float.contains("\"gap\":"), "{out}");

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

    // The interval world encloses: each answer is a binary64 interval
    // that contains the exact answer, so beside the exact worlds the
    // verdict is `agree` even where every float world disagrees.
    let out = compare_in("tenth", "n=10", &["exact", "interval"]);
    assert_eq!(verdict(&out), "agree", "{out}");
    let enclosure = world_row(&out, "interval");
    assert!(
        enclosure.contains("\"representation\":\"enclosure\""),
        "{out}"
    );
    assert!(
        enclosure.contains("\"payload\":\"[0.09999999999999999,0.1]\""),
        "{out}"
    );
    // The nearest binary64 to 1/3 lies below it, the one to 1/10 above:
    // both enclosures step outward on the correct side.
    let out = compare_in("tenth", "n=3", &["exact", "interval"]);
    assert_eq!(verdict(&out), "agree", "{out}");
    assert!(
        world_row(&out, "interval")
            .contains("\"payload\":\"[0.3333333333333333,0.33333333333333337]\""),
        "{out}"
    );
    // A decimal input reads as the tightest interval around its exact value.
    let out = compare_in("keep", "x=0.1", &["interval"]);
    assert!(
        world_row(&out, "interval").contains("\"payload\":\"[0.09999999999999999,0.1]\""),
        "{out}"
    );
    let out = compare_in("keep", "x=0.3", &["interval"]);
    assert!(
        world_row(&out, "interval").contains("\"payload\":\"[0.3,0.30000000000000004]\""),
        "{out}"
    );
    // A divisor enclosure that contains zero refuses by name.
    let out = compare_in("tenth", "n=0", &["interval"]);
    assert!(
        world_row(&out, "interval").contains("\"code\":\"division_by_zero\""),
        "{out}"
    );
    let out = compare_in("cancellation", "x=0.1", &["reference", "interval"]);
    assert_eq!(verdict(&out), "agree", "{out}");
    // Four RK4 steps with every data operation rounded outward still
    // enclose the exact rational.
    let out = compare_in("grow", "steps=4", &["exact", "interval"]);
    assert_eq!(verdict(&out), "agree", "{out}");
    assert!(world_row(&out, "interval").contains("\"width\":"), "{out}");
    // A range input: x*x - 2 over [1.4, 1.5].
    let out = compare_in("near_root", "x=[1.4,1.5]", &["interval"]);
    let enclosure = world_row(&out, "interval");
    assert!(
        enclosure.contains("\"payload\":\"[-0.0400000000000") && enclosure.contains(",0.25]\""),
        "{out}"
    );
    let out = compare_in("near_root", "x=[1.5,1.4]", &["interval"]);
    assert!(
        world_row(&out, "interval").contains("\"code\":\"invalid_input\""),
        "{out}"
    );
    // `x + 0.2 == 0.3` with x around 0.1: the enclosures overlap, so the
    // branch is not decided and the world refuses instead of guessing.
    let out = compare_in("crosses", "x=0.1", &["exact", "interval"]);
    assert_eq!(verdict(&out), "partial", "{out}");
    assert!(
        world_row(&out, "interval").contains("\"code\":\"undecided_comparison\""),
        "{out}"
    );

    // A grid evaluates every point in every world and maps where they
    // diverge. 1/n + 1/n + 1/n == 3/n fails in binary64 exactly at n = 5
    // and n = 10 among 1..=12.
    let out = compare_args(
        "thirds_add_up",
        &[
            "--grid", "n=1..12", "--world", "exact", "--world", "float64",
        ],
    );
    let compact: String = out.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        compact.contains("\"schema_version\":\"emath.compare.grid.v1\""),
        "{out}"
    );
    assert!(compact.contains("\"points\":12"), "{out}");
    assert!(compact.contains("\"agree\":10"), "{out}");
    assert!(compact.contains("\"disagree\":2"), "{out}");
    assert_eq!(divergent_inputs(&compact, "n"), ["5", "10"], "{out}");
    // `LO..HI/N` steps exactly: x = 1/10, 2/10, ..., 1 read as exact
    // decimals, so the reference world's answer is exactly 0 everywhere
    // and float64 leaves a residue at six of the ten points.
    let out = compare_args(
        "cancellation",
        &[
            "--grid",
            "x=0.1..1/9",
            "--world",
            "reference",
            "--world",
            "float64",
        ],
    );
    let compact: String = out.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains("\"points\":10"), "{out}");
    assert_eq!(
        divergent_inputs(&compact, "x"),
        ["0.1", "0.2", "0.3", "0.4", "0.6", "0.9"],
        "{out}"
    );
    let worst = &compact[compact.find("\"worst\":").expect("worst point")..];
    assert!(worst.contains("\"max_gap_world\":\"float64\""), "{out}");
    // The largest residue, 2^-53, first appears at x = 0.4 (ties at 0.6
    // and 0.9 keep the earliest point).
    assert!(worst.contains("\"inputs\":{\"x\":\"0.4\"}"), "{out}");

    // quote.world: a function value evaluated under another world from
    // inside the language. The judge sees float64 part from exact at
    // n = 5 and agree at n = 4.
    let out = compare_in("judge", "n=5", &["reference"]);
    assert!(world_row(&out, "reference").contains("\"payload\":\"true\""), "{out}");
    let out = compare_in("judge", "n=4", &["reference"]);
    assert!(world_row(&out, "reference").contains("\"payload\":\"false\""), "{out}");
    // The argument 1/10 is read as float32 data inside; outside, the
    // float64 world resumes for `x / 3`, and the exact world stays exact.
    let out = compare_in("inside_out", "x=0.1", &["exact", "float64"]);
    assert!(
        world_row(&out, "exact").contains("\"payload\":\"(0.10000000149011612,1/30)\""),
        "{out}"
    );
    assert!(
        world_row(&out, "float64").contains("\"payload\":\"(0.10000000149011612,0.03333333333333333)\""),
        "{out}"
    );
    // A refusal inside is reported as the inner answer.
    let out = compare_in("inner_refusal", "x=0.1", &["reference"]);
    assert!(
        world_row(&out, "reference").contains("\"payload\":\"(false,type)\""),
        "{out}"
    );
    // `emath run --worlds` sets the list a judge reads; a bad name refuses.
    let run = |worlds: &[&str]| {
        let out = Command::new(emath_bin())
            .arg("run")
            .arg(workspace().join("tests/fixtures/compare/worlds.emath"))
            .args(["--function", "configured", "--set", "unused=0"])
            .args(worlds)
            .env("EMATH_ROOT", workspace())
            .output()
            .expect("run emath run");
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        (out.status.code(), text)
    };
    let (code, text) = run(&["--worlds", "exact,float64"]);
    assert!(code == Some(0) && text.trim() == "[exact, float64]", "{code:?} {text}");
    let (code, text) = run(&["--worlds", "exact,float65"]);
    assert!(code != Some(0) && text.contains("float65"), "{code:?} {text}");
    // A Rat past binary64 range on both sides still widens to its value.
    let out = Command::new(emath_bin())
        .arg("run")
        .arg(workspace().join("tests/fixtures/compare/worlds.emath"))
        .args(["--function", "huge_ratio", "--set", "n=400"])
        .env("EMATH_ROOT", workspace())
        .output()
        .expect("run emath run");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.code() == Some(0) && text.trim() == "3", "{text}");
}

/// The values of input `name` at the divergent points, in grid order.
fn divergent_inputs(compact: &str, name: &str) -> Vec<String> {
    let start = compact.find("\"divergent\":[").expect("divergent list");
    let end = compact[start..]
        .find("\"worst\":")
        .map_or(compact.len(), |i| start + i);
    let key = format!("\"inputs\":{{\"{name}\":\"");
    compact[start..end]
        .match_indices(&key)
        .map(|(at, _)| {
            let value = &compact[start + at + key.len()..];
            value[..value.find('"').expect("closing quote")].to_string()
        })
        .collect()
}

#![forbid(unsafe_code)]

//! emath capstone demos: `cargo xtask demo affine-scorer`,
//! `cargo xtask demo semantic-genesis`, and
//! `cargo xtask demo holes-synthesis`.
//!
//! The compiler demos run the real pipeline through the `emath` CLI and
//! verify deterministic artifacts plus negative controls. The holes
//! demo runs finite table synthesis in-process. Std-only.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use emath_holes::{SynthesisLaw, impossible_identity_laws, synthesize_tables};
use emath_term::SymbolId;

const REFERENCE_SOURCE: &str = "language/examples/01_arbitrary_glyphs.emath";
const GENERATED_DIR: &str = "examples/generated/semantic-genesis-worlds";

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let code = if args.first().map(String::as_str) == Some("demo") {
        match args.get(1).map(String::as_str) {
            Some("affine-scorer") => demo_affine_scorer(),
            Some("semantic-genesis") => demo_semantic_genesis(),
            Some("holes-synthesis") => demo_holes_synthesis(),
            Some("all") => {
                let slice = demo_affine_scorer();
                if slice != 0 {
                    slice
                } else {
                    demo_semantic_genesis()
                }
            }
            other => {
                eprintln!(
                    "unknown demo {other:?}; usage: cargo xtask demo <affine-scorer|semantic-genesis|holes-synthesis|all>"
                );
                2
            }
        }
    } else {
        eprintln!("usage: cargo xtask demo <affine-scorer|semantic-genesis|holes-synthesis|all>");
        2
    };
    std::process::exit(i32::from(code));
}

fn demo_affine_scorer() -> u8 {
    println!("== demo affine-scorer ==");
    let work = TempWork::new("emath-xtask-affine");
    match run_demo_affine_scorer(work.path()) {
        Ok(()) => {
            println!("affine-scorer demo: ok");
            0
        }
        Err(error) => {
            eprintln!("affine-scorer demo FAILED: {error}");
            1
        }
    }
}

fn run_demo_affine_scorer(work: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))?;
    // Full V5 pipeline with verification.
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "build",
            "tests/valid/affine_scorer.emath",
            "--out",
            &work.display().to_string(),
            "--verify",
        ]),
        "artifact build --verify",
    )?;
    // Host promotion with constructor invariants + negative control.
    let host = check(
        cargo_run(&["run", "-q", "-p", "demo-host"]),
        "demo-host run",
    )?;
    let host_out = String::from_utf8(host.stdout).map_err(|error| error.to_string())?;
    require(&host_out, "host integration ok", "demo-host final line")?;
    // Derived oracle, not a magic constant: the committed generated crate
    // is `AffineScorer::new(2.0, 1.0)` from tests/valid/affine_scorer.emath,
    // so score(3.0) = 2.0*3.0 + 1.0 = 7.0 by the published definitions.
    require(&host_out, "score(3.0) = 7", "demo-host score print")?;
    require(&host_out, "negative control", "demo-host negative control")?;
    Ok(())
}

fn demo_semantic_genesis() -> u8 {
    println!("== demo semantic-genesis ==");
    let work = TempWork::new("emath-xtask-sg");
    match run_demo_semantic_genesis(work.path()) {
        Ok(()) => {
            println!("semantic-genesis demo: ok");
            0
        }
        Err(error) => {
            eprintln!("semantic-genesis demo FAILED: {error}");
            1
        }
    }
}

const GENESIS_ARTIFACTS: [&str; 9] = [
    "source-artifact.json",
    "parse-forest.json",
    "signature.json",
    "free-term.json",
    "meaning-problem.json",
    "interpretation-portfolio.json",
    "world-admission.jsonl",
    "answer-receipt.json",
    "csa-baseline.json",
];

fn run_demo_semantic_genesis(work: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))?;
    let a = work.join("a");
    let b = work.join("b");
    let generated = work.join("generated");
    let source_arg = REFERENCE_SOURCE;

    // Full analysis pipeline, twice, on identical sources.
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "genesis",
            source_arg,
            "--out",
            &a.display().to_string(),
        ]),
        "genesis run #1",
    )?;
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "genesis",
            source_arg,
            "--out",
            &b.display().to_string(),
        ]),
        "genesis run #2",
    )?;
    for name in GENESIS_ARTIFACTS {
        require_bytes(&a.join(name), "genesis artifact")?;
    }
    let candidates = a.join("world-candidates");
    let entry_count = std::fs::read_dir(&candidates)
        .map_err(|error| error.to_string())?
        .count();
    if entry_count != 3 {
        return Err(format!("expected 3 world candidates, got {entry_count}"));
    }
    // G4 exit gate: the interpretation portfolio carries at least five
    // world classes, each with a deterministic identity.
    let portfolio = std::fs::read_to_string(a.join("interpretation-portfolio.json"))
        .map_err(|error| error.to_string())?;
    let world_count = portfolio.matches("\"world_id\"").count();
    if world_count < 5 {
        return Err(format!(
            "expected at least 5 portfolio world classes, got {world_count}"
        ));
    }
    diff_dirs(&a, &b, "genesis determinism")?;

    // SG-09 receipt closure: independently re-extract every bound field
    // from the answer receipt and recompute the receipt identity. The
    // preimage below must stay in sync with the emitter in
    // crates/emath-cli/src/genesis_cmd.rs.
    let receipt = std::fs::read_to_string(a.join("answer-receipt.json"))
        .map_err(|error| error.to_string())?;
    let receipt_preimage = |result: &str| -> Result<String, String> {
        Ok(format!(
            "receipt:v2:{}:{}:{}:{}:{}:{}:{}:{result}:{}:{}:{}:{}:{}",
            json_str(&receipt, "answer_id")?,
            json_u64(&receipt, "source_hash")?,
            json_u64(&receipt, "parse_id")?,
            json_u64(&receipt, "signature_id")?,
            json_u64(&receipt, "term_id")?,
            json_str(&receipt, "world_id")?,
            json_str(&receipt, "valuation")?,
            json_str(&receipt, "artifact_hash")?,
            json_str(&receipt, "portfolio_hash")?,
            json_str(&receipt, "trace_hash")?,
            json_str(&receipt, "authority")?,
            json_u64(&receipt, "vm_steps")?,
        ))
    };
    let bound_result = json_str(&receipt, "result")?;
    let recomputed = format!(
        "{:016x}",
        emath_world_ir::fnv1a64(receipt_preimage(&bound_result)?.as_bytes())
    );
    let receipt_id = json_str(&receipt, "receipt_id")?;
    if receipt_id != recomputed {
        return Err(format!(
            "answer receipt does not self-verify: receipt_id {receipt_id} != recomputed {recomputed}"
        ));
    }
    if json_str(&receipt, "artifact_hash")? == "0000000000000000" {
        return Err("answer receipt binds no code artifact (artifact_hash is 0)".to_string());
    }
    // Tamper negative control: a receipt whose result field was altered
    // must fail recomputation instead of passing silently.
    let tampered = format!(
        "{:016x}",
        emath_world_ir::fnv1a64(receipt_preimage("tampered-result")?.as_bytes())
    );
    if tampered == receipt_id {
        return Err("tampered receipt still verified; receipt binding is broken".to_string());
    }
    println!("answer receipt self-verifies: {receipt_id} (tamper control refused)");

    // VM answers the generated Rust must reproduce (SG-09 differential).
    let vm_free = portfolio_answer(&portfolio, "free_symbolic")?;
    let vm_boolean = portfolio_answer(&portfolio, "Boolean_algebra")?;
    let vm_modular = portfolio_answer(&portfolio, "modular_numeric")?;

    // Parametric codegen onto a fresh directory.
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "compile",
            "--parametric",
            source_arg,
            "--out",
            &generated.display().to_string(),
        ]),
        "compile --parametric",
    )?;
    for name in [
        "Cargo.toml",
        "src/lib.rs",
        "src/main.rs",
        "manifest.json",
        "source-map.json",
        "hole-manifest.json",
    ] {
        require_bytes(&generated.join(name), "generated file")?;
    }
    // The committed generated crate must be byte-identical (the CLI-only
    // manifest/source-map/hole-manifest artifacts are not part of the
    // committed crate).
    diff_dirs_excluding(
        &generated,
        Path::new(GENERATED_DIR),
        "committed generated crate fidelity",
        &["manifest.json", "source-map.json", "hole-manifest.json"],
    )?;

    // Generated crate tests: fixtures + wrong-world negative control.
    let test = Command::new("cargo")
        .args(["test", "--quiet", "--manifest-path"])
        .arg(generated.join("Cargo.toml"))
        .output()
        .map_err(|error| error.to_string())?;
    if !test.status.success() {
        let stderr = String::from_utf8_lossy(&test.stderr);
        return Err(format!("generated crate tests failed:\n{stderr}"));
    }

    // Runtime differential evaluation through the generated binary.
    let run = Command::new("cargo")
        .args(["run", "--quiet", "--manifest-path"])
        .arg(generated.join("Cargo.toml"))
        .output()
        .map_err(|error| error.to_string())?;
    if !run.status.success() {
        let stderr = String::from_utf8_lossy(&run.stderr);
        return Err(format!("generated crate run failed:\n{stderr}"));
    }
    let stdout = String::from_utf8(run.stdout).map_err(|error| error.to_string())?;
    let mut free = None;
    let mut boolean = None;
    let mut modular = None;
    let mut swapped = None;
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix("free: ") {
            free = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("boolean: ") {
            boolean = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("modular-17: ") {
            modular = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("swapped-modular-17: ") {
            swapped = Some(value.to_string());
        }
    }
    let free = free.ok_or("missing free: line")?;
    let boolean = boolean.ok_or("missing boolean: line")?;
    let modular = modular.ok_or("missing modular-17: line")?;
    let swapped = swapped.ok_or("missing swapped-modular-17: line")?;
    if free != "apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))" {
        return Err(format!("free-symbolic result wrong: {free}"));
    }
    if boolean != "false" {
        return Err(format!("boolean result wrong: {boolean}"));
    }
    if modular != "6" {
        return Err(format!("modular-17 result wrong: {modular}"));
    }
    // Derived oracle, not a magic constant: with a=4, b=7, ζ=3 and
    // modular-17 arithmetic, the term ⊛(⧖(⋈(a, b)), ζ) is
    // ⋈(4,7)=11 → ⧖(11)=121 mod 17=2 → ⊛(2,3)=6 under the modular world,
    // and ⋈(4,7)=28 mod 17=11 → ⧖(11)=2 → ⊛(2,3)=2+3=5 under the swapped
    // world (+ ↔ ×). The negative control must produce exactly 5; a
    // wrong world (or a no-op swap mutant) collides with the oracle 6.
    if swapped != "5" {
        return Err(format!(
            "wrong world not rejected: swapped = {swapped}, expected 5"
        ));
    }
    // SG-09 VM/Rust differential: the generated Rust must reproduce the
    // semantic VM's own answers from the interpretation portfolio, not
    // merely the static oracle pins above.
    for (world, vm_answer, rust_answer) in [
        ("free_symbolic", &vm_free, &free),
        ("Boolean_algebra", &vm_boolean, &boolean),
        ("modular_numeric", &vm_modular, &modular),
    ] {
        if vm_answer != rust_answer {
            return Err(format!(
                "VM/Rust differential failed for {world}: VM answered {vm_answer}, generated Rust answered {rust_answer}"
            ));
        }
    }
    println!("free: {free}");
    println!("boolean: {boolean}");
    println!("modular-17: {modular}");
    println!("swapped-modular-17: {swapped} (distinct oracle pin; no-op swap rejected)");
    println!("vm/rust differential: 3 worlds agree with the semantic VM");
    Ok(())
}

/// Extracts a string field `"name": "value"` from single-object JSON
/// written by the emath-artifact writer (no escaped quotes in the values
/// this verifier reads).
fn json_str(json: &str, name: &str) -> Result<String, String> {
    let needle = format!("\"{name}\": \"");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing string field {name}"))?
        + needle.len();
    let end = json[start..]
        .find('"')
        .ok_or_else(|| format!("unterminated string field {name}"))?;
    Ok(json[start..start + end].to_string())
}

/// Extracts an integer field `"name": value` from single-object JSON.
fn json_u64(json: &str, name: &str) -> Result<u64, String> {
    let needle = format!("\"{name}\": ");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing int field {name}"))?
        + needle.len();
    let digits: String = json[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse::<u64>()
        .map_err(|error| format!("field {name} is not a u64: {error}"))
}

/// The recorded VM answer for `world` inside the interpretation
/// portfolio's inline candidate array.
fn portfolio_answer(portfolio: &str, world: &str) -> Result<String, String> {
    let needle = format!("\"name\":\"{world}\",\"answer\":\"");
    let start = portfolio
        .find(&needle)
        .ok_or_else(|| format!("portfolio has no candidate named {world}"))?
        + needle.len();
    let end = portfolio[start..]
        .find('"')
        .ok_or_else(|| format!("unterminated answer for {world}"))?;
    Ok(portfolio[start..start + end].to_string())
}

fn demo_holes_synthesis() -> u8 {
    println!("== demo holes-synthesis ==");
    match run_demo_holes_synthesis() {
        Ok(()) => {
            println!("holes-synthesis demo: ok");
            0
        }
        Err(error) => {
            eprintln!("holes-synthesis demo FAILED: {error}");
            1
        }
    }
}

fn run_demo_holes_synthesis() -> Result<(), String> {
    let op = SymbolId("op".to_string());
    let carrier = ["0".to_string(), "1".to_string()];
    let budget = 2_u64.pow(4);
    let monoid = [
        SynthesisLaw::Identity(op.clone(), SymbolId("0".to_string())),
        SynthesisLaw::Associative(op.clone()),
        SynthesisLaw::Commutative(op.clone()),
    ];
    let found = synthesize_tables(&op, &carrier, &monoid, budget)
        .map_err(|error| format!("monoid synthesis refused: {error:?}"))?;
    println!(
        "holes-synthesis: commutative-monoid candidates={} exhaustive={} examined={}",
        found.tables.len(),
        found.exhaustive,
        found.examined
    );
    if found.tables.is_empty() || !found.exhaustive {
        return Err(format!(
            "expected exhaustive commutative-monoid tables, got candidates={} exhaustive={}",
            found.tables.len(),
            found.exhaustive
        ));
    }

    let rejected = synthesize_tables(&op, &carrier, &impossible_identity_laws(&op), budget)
        .map_err(|error| format!("impossible-identity synthesis refused: {error:?}"))?;
    println!(
        "holes-synthesis: impossible-identity rejected candidates={} exhaustive={} examined={}",
        rejected.tables.len(),
        rejected.exhaustive,
        rejected.examined
    );
    if !rejected.tables.is_empty() || !rejected.exhaustive {
        return Err(format!(
            "expected exhaustive reject of two identities, got candidates={} exhaustive={}",
            rejected.tables.len(),
            rejected.exhaustive
        ));
    }
    Ok(())
}

fn cargo_run(args: &[&str]) -> Output {
    Command::new("cargo")
        .args(args)
        .output()
        .expect("cargo spawns")
}

fn check(output: Output, what: &str) -> Result<Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{what} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn require(haystack: &str, needle: &str, what: &str) -> Result<(), String> {
    if haystack.contains(needle) {
        Ok(())
    } else {
        Err(format!("{what}: missing `{needle}` in:\n{haystack}"))
    }
}

fn require_bytes(path: &Path, what: &str) -> Result<(), String> {
    if std::fs::read(path).is_ok_and(|bytes| !bytes.is_empty()) {
        Ok(())
    } else {
        Err(format!("{what}: missing or empty {}", path.display()))
    }
}

/// RAII temp workdir: `$TMPDIR/<label>-<pid>` is removed on drop, so demo
/// runs never leak `emath-xtask-*` directories.
struct TempWork(PathBuf);

impl TempWork {
    fn new(label: &str) -> Self {
        Self(std::env::temp_dir().join(format!("{label}-{}", std::process::id())))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempWork {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn diff_dirs(left: &Path, right: &Path, what: &str) -> Result<(), String> {
    diff_dirs_excluding(left, right, what, &[])
}

fn diff_dirs_excluding(
    left: &Path,
    right: &Path,
    what: &str,
    exclude: &[&str],
) -> Result<(), String> {
    let mut left_files = Vec::new();
    collect_files(left, &mut left_files)?;
    let mut right_files = Vec::new();
    collect_files(right, &mut right_files)?;
    let keep = |relative: &String| !exclude.contains(&relative.as_str());
    let left_files = left_files.into_iter().filter(keep).collect::<Vec<_>>();
    let right_files = right_files.into_iter().filter(keep).collect::<Vec<_>>();
    if left_files != right_files {
        return Err(format!(
            "{what}: file sets differ\nleft:  {left_files:?}\nright: {right_files:?}"
        ));
    }
    for relative in left_files {
        let l = std::fs::read(left.join(&relative)).map_err(|e| e.to_string())?;
        let r = std::fs::read(right.join(&relative)).map_err(|e| e.to_string())?;
        if l != r {
            return Err(format!("{what}: {relative} differs"));
        }
    }
    Ok(())
}

fn collect_files(root: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let mut queue = vec![root.to_path_buf()];
    while let Some(dir) = queue.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned();
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(())
}

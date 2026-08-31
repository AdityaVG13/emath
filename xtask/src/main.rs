#![forbid(unsafe_code)]

//! emath capstone demos: `cargo xtask demo affine-scorer`,
//! `cargo xtask demo semantic-genesis`, `cargo xtask demo holes-synthesis`,
//! `cargo xtask demo scoped-binders`, and `cargo xtask demo math-layout`.
//!
//! The compiler demos run the real pipeline through the `emath` CLI and
//! verify deterministic artifacts plus negative controls. The holes
//! demo runs finite table synthesis in-process; the scoped-binders demo
//! runs SG-10 binder expansion through the semantic VM and emits a
//! deterministic receipt; the math-layout demo runs SG-11/SG-12 LaTeX and
//! PDF-fixture frontends into the shared layout graph. Std-only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use emath_genesis::{
    binder_id,
    vm::{VmBudget, VmOutcome},
    BinderBudget, BinderDomain, BinderError, BinderFamily, BinderKind, BinderTerm, FreeTermWorld,
    ScopedBinder, BINDER_SCHEMA, BINDER_VERSION,
};
use emath_holes::{impossible_identity_laws, synthesize_tables, SynthesisLaw};
use emath_layout::{
    extract, parse_latex, reference_fixture, to_binder_term, LayoutError, PdfPageFixture,
    PositionedGlyph, LAYOUT_SCHEMA, LAYOUT_VERSION,
};
use emath_term::{SymbolId, Term, VariableId};
use emath_world_ir::fnv1a64;

mod demo_agent_meaning;
mod demo_finite_analogues;
mod demo_finite_worlds;
mod demo_interpretation_portfolio;
mod demo_joint_tuning;
mod demo_meaning_store;
mod demo_portable_emlib;
mod demo_source_first_worlds;
mod demo_world_morphisms;

const REFERENCE_SOURCE: &str = "tests/valid/arbitrary-glyphs.emath";
const GENERATED_DIR: &str = "examples/generated/semantic-genesis-worlds";

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let code = if args.first().map(String::as_str) == Some("demo") {
        match args.get(1).map(String::as_str) {
            Some("affine-scorer") => demo_affine_scorer(),
            Some("semantic-genesis") => demo_semantic_genesis(),
            Some("holes-synthesis") => demo_holes_synthesis(),
            Some("scoped-binders") => demo_scoped_binders(),
            Some("math-layout") => demo_math_layout(),
            Some("interpretation-portfolio") => demo_interpretation_portfolio::run(),
            Some("finite-analogues") => demo_finite_analogues::demo(),
            Some("finite-worlds") => demo_finite_worlds::demo(),
            Some("agent-meaning") => demo_agent_meaning::demo(),
            Some("world-morphisms") => demo_world_morphisms::demo(),
            Some("joint-tuning") => demo_joint_tuning::demo(),
            Some("source-first-worlds") => demo_source_first_worlds::demo(),
            Some("meaning-store") => demo_meaning_store::demo(),
            Some("portable-emlib") => demo_portable_emlib::demo(),
            Some("cache-policy") => demo_cache_policy(),
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
                    "unknown demo {other:?}; usage: cargo xtask demo <affine-scorer|semantic-genesis|holes-synthesis|scoped-binders|math-layout|interpretation-portfolio|finite-analogues|finite-worlds|agent-meaning|world-morphisms|joint-tuning|source-first-worlds|cache-policy|all>"
                );
                2
            }
        }
    } else if args.first().map(String::as_str) == Some("build-web") {
        build_web()
    } else if args.first().map(String::as_str) == Some("check-wasm") {
        check_wasm()
    } else if args.first().map(String::as_str) == Some("serve-web")
        || args.first().map(String::as_str) == Some("serve")
    {
        let port = args
            .get(1)
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);
        serve_web(port)
    } else {
        eprintln!(
            "usage: cargo xtask demo <affine-scorer|semantic-genesis|holes-synthesis|scoped-binders|math-layout|interpretation-portfolio|finite-analogues|finite-worlds|agent-meaning|world-morphisms|joint-tuning|source-first-worlds|cache-policy|all>"
        );
        eprintln!("       cargo xtask build-web");
        eprintln!("       cargo xtask check-wasm");
        eprintln!("       cargo xtask serve-web [port]");
        2
    };
    std::process::exit(i32::from(code));
}

fn demo_cache_policy() -> u8 {
    println!("== demo cache-policy ==");
    let work = TempWork::new("emath-xtask-cache-policy");
    match run_demo_cache_policy(work.path()) {
        Ok(()) => {
            println!("cache-policy demo: ok");
            0
        }
        Err(error) => {
            eprintln!("cache-policy demo FAILED: {error}");
            1
        }
    }
}

fn run_demo_cache_policy(work: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))?;
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "check",
            "language/examples/integration/cache-policy.emath",
        ]),
        "cache-policy check",
    )?;
    let built = check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "build",
            "language/examples/integration/cache-policy.emath",
            "--out",
            &work.display().to_string(),
        ]),
        "cache-policy build",
    )?;
    let stdout = String::from_utf8(built.stdout).map_err(|error| error.to_string())?;
    require(&stdout, "AdaptiveCachePolicy", "generated crate name")?;
    require(&stdout, "artifact fnv1a64:", "artifact identity")?;
    println!("cache-policy admit: ok");
    println!("cache-policy build: ok");
    println!("cache-policy host-impl: retained, not emitted (typed no-claim)");
    Ok(())
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

const GENESIS_ARTIFACTS: [&str; 10] = [
    "source-artifact.json",
    "parse-forest.json",
    "signature.json",
    "free-term.json",
    "meaning-problem.json",
    "interpretation-portfolio.json",
    "g7-portfolio-receipt.txt",
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

fn demo_scoped_binders() -> u8 {
    println!("== demo scoped-binders ==");
    let work = TempWork::new("emath-xtask-binders");
    match run_demo_scoped_binders(work.path()) {
        Ok(()) => {
            println!("scoped-binders demo: ok");
            0
        }
        Err(error) => {
            eprintln!("scoped-binders demo FAILED: {error}");
            1
        }
    }
}

/// SG-10 production path: builds all six binder kinds across the four
/// families, expands the expandable ones, evaluates the expansions through
/// the semantic VM, records the typed refusals, and emits a deterministic
/// machine-readable receipt with a seeded tamper control.
fn run_demo_scoped_binders(work: &Path) -> Result<(), String> {
    std::fs::create_dir_all(work).map_err(|error| format!("create work dir: {error}"))?;
    emath_genesis::binder::check_version(BINDER_VERSION)
        .map_err(|error| format!("binder version handshake refused: {error:?}"))?;

    let variable = |name: &str| VariableId(name.to_string());
    let free = |name: &str| Term::Variable(variable(name));
    let plus = SymbolId("+".to_string());
    let times = SymbolId("*".to_string());
    let make =
        |kind: BinderKind, family: BinderFamily, domain: BinderDomain, bound: &str, body: Term| {
            ScopedBinder {
                kind,
                family,
                domain,
                bound: variable(bound),
                body: BinderTerm::Leaf(body),
            }
        };

    // Expandable binders: structural sum/product/custom, finite-analogue
    // integral (Riemann-style analogue; no continuum claim).
    let expandable = [
        (
            "sum",
            make(
                BinderKind::Sum,
                BinderFamily::Structural,
                BinderDomain::FiniteRange { lower: 1, upper: 3 },
                "x",
                free("x"),
            ),
            plus.clone(),
        ),
        (
            "product",
            make(
                BinderKind::Product,
                BinderFamily::Structural,
                BinderDomain::FiniteRange { lower: 1, upper: 3 },
                "x",
                free("x"),
            ),
            times.clone(),
        ),
        (
            "integral-finite-analogue",
            make(
                BinderKind::Integral,
                BinderFamily::FiniteAnalogue,
                BinderDomain::FiniteRange { lower: 0, upper: 4 },
                "t",
                free("t"),
            ),
            plus.clone(),
        ),
        (
            "custom-bigjoin",
            make(
                BinderKind::Custom("bigjoin".to_string()),
                BinderFamily::Structural,
                BinderDomain::FiniteRange { lower: 1, upper: 2 },
                "k",
                free("k"),
            ),
            plus.clone(),
        ),
    ];

    let mut rows: Vec<String> = Vec::new();
    let environment: emath_genesis::Environment<Term> = BTreeMap::new();
    for (label, binder, combine) in &expandable {
        let expanded = binder
            .expand(combine, BinderBudget::default())
            .map_err(|error| format!("{label}: expansion refused: {error:?}"))?;
        let outcome = emath_genesis::run(
            &expanded,
            &FreeTermWorld,
            &environment,
            &VmBudget { max_steps: 1024 },
        )
        .map_err(|error| format!("{label}: vm evaluation failed: {error:?}"))?;
        let VmOutcome::Complete { value, steps, .. } = outcome else {
            return Err(format!("{label}: vm suspended on a tiny term"));
        };
        rows.push(format!(
            "expand|{label}|id={:016x}|steps={steps}|vm={}",
            binder_id(binder),
            value.canonical()
        ));
    }

    // Conventional derivative: expansion is a typed refusal.
    let derivative = make(
        BinderKind::Derivative,
        BinderFamily::Conventional,
        BinderDomain::Symbolic {
            anchor: "t".to_string(),
        },
        "t",
        free("t"),
    );
    match derivative.expand(&plus, BinderBudget::default()) {
        Err(BinderError::NotExpandable { kind, family }) => {
            rows.push(format!(
                "refuse|derivative|not-expandable|{kind}|{}",
                family.canonical()
            ));
        }
        other => {
            return Err(format!(
                "conventional derivative must refuse, got {other:?}"
            ))
        }
    }

    // Opaque-seeded limit: deterministic seeded identity, seed-sensitive.
    let limit = make(
        BinderKind::Limit,
        BinderFamily::OpaqueSeeded,
        BinderDomain::Symbolic {
            anchor: "x->0".to_string(),
        },
        "x",
        free("x"),
    );
    let seed_a = limit
        .opaque_identity(1)
        .map_err(|error| format!("opaque identity refused: {error:?}"))?;
    let seed_a_again = limit
        .opaque_identity(1)
        .map_err(|error| format!("opaque identity refused: {error:?}"))?;
    let seed_b = limit
        .opaque_identity(2)
        .map_err(|error| format!("opaque identity refused: {error:?}"))?;
    if seed_a != seed_a_again || seed_a == seed_b {
        return Err("opaque identity must be seed-deterministic and seed-sensitive".to_string());
    }
    rows.push(format!(
        "opaque|limit|seed1={seed_a:016x}|seed2={seed_b:016x}"
    ));

    // Budget refusal: 1..=1000 under a budget of 8 instantiations.
    let big = make(
        BinderKind::Sum,
        BinderFamily::Structural,
        BinderDomain::FiniteRange {
            lower: 1,
            upper: 1000,
        },
        "x",
        free("x"),
    );
    match big.expand(&plus, BinderBudget { max_terms: 8 }) {
        Err(BinderError::BudgetExceeded { limit: 8 }) => {
            rows.push("refuse|sum-1000|budget-exceeded|limit=8".to_string());
        }
        other => return Err(format!("budget overrun must refuse, got {other:?}")),
    }

    // Alpha invariance: renaming the bound variable preserves identity.
    let alpha = make(
        BinderKind::Sum,
        BinderFamily::Structural,
        BinderDomain::FiniteRange { lower: 1, upper: 3 },
        "z",
        free("z"),
    );
    if binder_id(&alpha) != binder_id(&expandable[0].1) {
        return Err("alpha-equivalent binders must share one identity".to_string());
    }
    rows.push(format!("alpha|sum|id={:016x}", binder_id(&alpha)));

    // Deterministic machine-readable receipt.
    let body = rows.join("\n");
    let receipt_id = fnv1a64(body.as_bytes());
    let artifact = format!(
        "{{\"schema\":\"{BINDER_SCHEMA}\",\"version\":{BINDER_VERSION},\"rows\":[{}],\"receipt_id\":\"{receipt_id:016x}\"}}\n",
        rows.iter()
            .map(|row| format!("\"{}\"", row.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(",")
    );
    let path = work.join("scoped-binders.json");
    std::fs::write(&path, &artifact).map_err(|error| format!("write receipt: {error}"))?;
    let reread = std::fs::read_to_string(&path).map_err(|error| format!("reread: {error}"))?;
    if reread != artifact {
        return Err("receipt must round-trip byte-exactly".to_string());
    }

    // Seeded negative control: a tampered receipt body must change the id.
    let tampered = body.replacen("id=", "id=f", 1);
    if tampered == body || fnv1a64(tampered.as_bytes()) == receipt_id {
        return Err("tamper control failed: receipt id did not change".to_string());
    }

    println!(
        "scoped-binders: rows={} receipt={receipt_id:016x} artifact={}",
        rows.len(),
        path.display()
    );
    Ok(())
}

fn demo_math_layout() -> u8 {
    println!("== demo math-layout ==");
    let work = TempWork::new("emath-xtask-layout");
    match run_demo_math_layout(work.path()) {
        Ok(()) => {
            println!("math-layout demo: ok");
            0
        }
        Err(error) => {
            eprintln!("math-layout demo FAILED: {error}");
            1
        }
    }
}

/// SG-11/SG-12 production path: parse a mixed LaTeX document, extract the
/// PDF reference fixture, lower the LaTeX sum through the semantic VM,
/// record a typed refusal and a retained ambiguity, and emit a
/// deterministic machine-readable receipt with a seeded tamper control.
fn run_demo_math_layout(work: &Path) -> Result<(), String> {
    std::fs::create_dir_all(work).map_err(|error| format!("create work dir: {error}"))?;
    emath_layout::check_version(LAYOUT_VERSION)
        .map_err(|error| format!("layout version handshake refused: {error:?}"))?;

    let latex_source = r"Let $\sum_{i=1}^{3} i$ be finite.";
    let latex_graph =
        parse_latex(latex_source).map_err(|error| format!("latex parse refused: {error:?}"))?;
    if latex_graph.source().as_bytes() != latex_source.as_bytes() {
        return Err("latex source must be preserved byte-exactly".to_string());
    }
    let binder_term =
        to_binder_term(&latex_graph).map_err(|error| format!("latex lower refused: {error:?}"))?;
    let BinderTerm::Bind(binder) = binder_term else {
        return Err(format!("expected a sum binder, got {binder_term:?}"));
    };
    let expanded = binder
        .expand(&SymbolId("+".to_string()), BinderBudget::default())
        .map_err(|error| format!("sum expansion refused: {error:?}"))?;
    let environment: emath_genesis::Environment<Term> = BTreeMap::new();
    let outcome = emath_genesis::run(
        &expanded,
        &FreeTermWorld,
        &environment,
        &VmBudget { max_steps: 1024 },
    )
    .map_err(|error| format!("vm evaluation failed: {error:?}"))?;
    let VmOutcome::Complete { value, steps, .. } = outcome else {
        return Err("vm suspended on a tiny term".to_string());
    };

    let mut rows: Vec<String> = Vec::new();
    rows.push(format!(
        "latex|sum|graph={:016x}|expand={}|steps={steps}|vm={}",
        latex_graph.graph_id(),
        expanded.canonical(),
        value.canonical()
    ));

    match parse_latex(r"\foo") {
        Err(LayoutError::UnknownMacro { name, offset }) => {
            rows.push(format!("refuse|unknown-macro|{name}|offset={offset}"));
        }
        other => return Err(format!("unknown macro must refuse, got {other:?}")),
    }

    let pdf_graph = extract(&reference_fixture());
    let supers = pdf_graph
        .edges()
        .iter()
        .filter(|edge| matches!(edge.relation, emath_layout::SpatialRelation::SuperscriptOf))
        .count();
    rows.push(format!(
        "pdf|reference|graph={:016x}|regions={}|supers={supers}",
        pdf_graph.graph_id(),
        pdf_graph.formula_regions().count()
    ));

    let ambiguous = PdfPageFixture {
        source_label: "demo-ambiguous".to_string(),
        glyphs: vec![
            PositionedGlyph {
                glyph: "x".to_string(),
                x: 0,
                y: 0,
                width: 800,
                height: 1000,
                font_size: 1000,
            },
            PositionedGlyph {
                glyph: "2".to_string(),
                x: 800,
                y: 300,
                width: 400,
                height: 600,
                font_size: 600,
            },
        ],
    };
    let amb_graph = extract(&ambiguous);
    let amb = amb_graph
        .ambiguities()
        .first()
        .ok_or_else(|| "expected a retained ambiguity".to_string())?;
    rows.push(format!(
        "ambiguity|node={}|a={}|b={}",
        amb.node_id.0, amb.reading_a, amb.reading_b
    ));

    let body = rows.join("\n");
    let receipt_id = fnv1a64(body.as_bytes());
    let artifact = format!(
        "{{\"schema\":\"{LAYOUT_SCHEMA}\",\"version\":{LAYOUT_VERSION},\"rows\":[{}],\"receipt_id\":\"{receipt_id:016x}\"}}\n",
        rows.iter()
            .map(|row| format!("\"{}\"", row.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(",")
    );
    let path = work.join("math-layout.json");
    std::fs::write(&path, &artifact).map_err(|error| format!("write receipt: {error}"))?;
    let reread = std::fs::read_to_string(&path).map_err(|error| format!("reread: {error}"))?;
    if reread != artifact {
        return Err("receipt must round-trip byte-exactly".to_string());
    }

    let tampered = body.replacen("graph=", "graph=f", 1);
    if tampered == body || fnv1a64(tampered.as_bytes()) == receipt_id {
        return Err("tamper control failed: receipt id did not change".to_string());
    }

    println!(
        "math-layout: rows={} receipt={receipt_id:016x} artifact={}",
        rows.len(),
        path.display()
    );
    Ok(())
}

/// rustc-check `emath-wasm` for `wasm32-unknown-unknown` so the target
/// does not rot. Not a test suite.
fn check_wasm() -> u8 {
    println!("== check-wasm ==");
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|list| {
            list.lines()
                .any(|line| line.trim() == "wasm32-unknown-unknown")
        });
    if !installed {
        eprintln!("check-wasm: rustup target wasm32-unknown-unknown is not installed");
        eprintln!("            rustup target add wasm32-unknown-unknown");
        return 1;
    }
    let status = Command::new("cargo")
        .args([
            "check",
            "-p",
            "emath-wasm",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .status();
    match status {
        Ok(status) if status.success() => {
            println!("check-wasm: emath-wasm wasm32-unknown-unknown ok");
            0
        }
        Ok(_) => {
            eprintln!(
                "check-wasm: cargo check -p emath-wasm --target wasm32-unknown-unknown failed"
            );
            1
        }
        Err(error) => {
            eprintln!("check-wasm: failed to spawn cargo: {error}");
            1
        }
    }
}

/// Build `emath-wasm` for `wasm32-unknown-unknown` and stage `web/dist/`.
fn build_web() -> u8 {
    println!("== build-web ==");
    let status = Command::new("cargo")
        .args([
            "build",
            "-p",
            "emath-wasm",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .status();
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            eprintln!("build-web: failed to spawn cargo: {error}");
            return 1;
        }
    };
    if !status.success() {
        eprintln!("build-web: cargo build -p emath-wasm failed");
        return 1;
    }
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let wasm_src = PathBuf::from(&target_dir)
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("emath_wasm.wasm");
    if !wasm_src.is_file() {
        eprintln!("build-web: missing {}", wasm_src.display());
        return 1;
    }
    let dist = PathBuf::from("web/dist");
    if let Err(error) = std::fs::create_dir_all(&dist) {
        eprintln!("build-web: cannot create {}: {error}", dist.display());
        return 1;
    }
    let wasm_dest = dist.join("emath.wasm");
    if let Err(error) = std::fs::copy(&wasm_src, &wasm_dest) {
        eprintln!(
            "build-web: cannot copy {} -> {}: {error}",
            wasm_src.display(),
            wasm_dest.display()
        );
        return 1;
    }
    let wasm_opt_available = Command::new("wasm-opt")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if wasm_opt_available {
        println!("build-web: running wasm-opt -O3 --strip-debug --strip-producers on emath.wasm");
        let opt_status = Command::new("wasm-opt")
            .args(["-O3", "--strip-debug", "--strip-producers", "-o"])
            .arg(&wasm_dest)
            .arg(&wasm_dest)
            .status();
        match opt_status {
            Ok(status) if status.success() => {
                println!("build-web: wasm-opt optimization complete");
            }
            Ok(status) => {
                eprintln!(
                    "build-web: warning: wasm-opt exited with status {:?}",
                    status.code()
                );
            }
            Err(error) => {
                eprintln!("build-web: warning: failed to run wasm-opt: {error}");
            }
        }
    } else {
        println!(
            "build-web: note: wasm-opt not found in PATH; skipping wasm-opt post-processing pass"
        );
    }
    for name in ["index.html", "app.js", "style.css"] {
        let src = PathBuf::from("web").join(name);
        if !src.is_file() {
            eprintln!(
                "build-web: note: {} not found; skipping",
                src.display()
            );
        }
    }

    // 1. WASM hash
    let wasm_bytes = match std::fs::read(&wasm_dest) {
        Ok(b) => b,
        Err(error) => {
            eprintln!("build-web: cannot read {}: {error}", wasm_dest.display());
            return 1;
        }
    };
    let wasm_hash = format!("{:012x}", fnv1a64(&wasm_bytes));

    // 2. CSS staging and content hash
    let css_src = PathBuf::from("web/style.css");
    let css_hash = if css_src.is_file() {
        let css_bytes = match std::fs::read(&css_src) {
            Ok(b) => b,
            Err(error) => {
                eprintln!("build-web: cannot read {}: {error}", css_src.display());
                return 1;
            }
        };
        if let Err(error) = std::fs::write(dist.join("style.css"), &css_bytes) {
            eprintln!("build-web: cannot write dist/style.css: {error}");
            return 1;
        }
        format!("{:012x}", fnv1a64(&css_bytes))
    } else {
        "0".to_string()
    };

    // 3. JS staging with stamped WASM URL and content hash
    let js_src = PathBuf::from("web/app.js");
    let js_hash = if js_src.is_file() {
        let js_str = match std::fs::read_to_string(&js_src) {
            Ok(s) => s,
            Err(error) => {
                eprintln!("build-web: cannot read {}: {error}", js_src.display());
                return 1;
            }
        };
        let stamped_wasm_target = format!("\"/emath.wasm?v={wasm_hash}\"");
        let stamped_js = js_str.replace("\"/emath.wasm\"", &stamped_wasm_target);
        if let Err(error) = std::fs::write(dist.join("app.js"), stamped_js.as_bytes()) {
            eprintln!("build-web: cannot write dist/app.js: {error}");
            return 1;
        }
        format!("{:012x}", fnv1a64(stamped_js.as_bytes()))
    } else {
        "0".to_string()
    };

    // 4. HTML staging with stamped CSS and JS references
    let html_src = PathBuf::from("web/index.html");
    if html_src.is_file() {
        let html_str = match std::fs::read_to_string(&html_src) {
            Ok(s) => s,
            Err(error) => {
                eprintln!("build-web: cannot read {}: {error}", html_src.display());
                return 1;
            }
        };
        let stamped_css_ref = format!("href=\"/style.css?v={css_hash}\"");
        let stamped_js_ref = format!("src=\"/app.js?v={js_hash}\"");
        let stamped_html = html_str
            .replace("href=\"/style.css\"", &stamped_css_ref)
            .replace("href=\"style.css\"", &stamped_css_ref)
            .replace("src=\"/app.js\"", &stamped_js_ref)
            .replace("src=\"app.js\"", &stamped_js_ref);
        if let Err(error) = std::fs::write(dist.join("index.html"), stamped_html.as_bytes()) {
            eprintln!("build-web: cannot write dist/index.html: {error}");
            return 1;
        }
    }

    println!("build-web: asset cache stamping:");
    println!("  emath.wasm  v={wasm_hash}");
    println!("  style.css   v={css_hash}");
    println!("  app.js      v={js_hash}");
    println!("build-web: wrote {}", dist.display());
    match std::fs::read_dir(&dist) {
        Ok(entries) => {
            let mut names = Vec::new();
            for entry in entries.flatten() {
                names.push(entry.path());
            }
            names.sort();
            let mut budget_failed = false;
            for path in names {
                let raw_size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                match compute_gzip_size(&path) {
                    Ok(gzip_size) => {
                        println!(
                            "  {}  {raw_size} bytes (raw) / {gzip_size} bytes (gzip)",
                            path.display()
                        );
                        if path.file_name().and_then(|n| n.to_str()) == Some("emath.wasm") {
                            const MAX_RAW_BYTES: u64 = 2 * 1024 * 1024; // 2.0 MB
                            const MAX_GZIP_BYTES: u64 = 500 * 1024; // 500 KB
                            if raw_size > MAX_RAW_BYTES {
                                eprintln!(
                                    "build-web: error: emath.wasm raw size {raw_size} exceeds budget of {MAX_RAW_BYTES} bytes"
                                );
                                budget_failed = true;
                            }
                            if gzip_size > MAX_GZIP_BYTES {
                                eprintln!(
                                    "build-web: error: emath.wasm gzip size {gzip_size} exceeds budget of {MAX_GZIP_BYTES} bytes"
                                );
                                budget_failed = true;
                            }
                        }
                    }
                    Err(error) => {
                        println!(
                            "  {}  {raw_size} bytes (raw) / gzip check skipped ({error})",
                            path.display()
                        );
                    }
                }
            }
            if budget_failed {
                return 1;
            }
        }
        Err(error) => eprintln!("build-web: cannot list {}: {error}", dist.display()),
    }
    0
}

fn compute_gzip_size(path: &Path) -> Result<u64, String> {
    let output = Command::new("gzip")
        .args(["-9", "-c"])
        .arg(path)
        .output()
        .map_err(|error| format!("failed to spawn gzip: {error}"))?;
    if !output.status.success() {
        return Err(format!("gzip failed with exit status {:?}", output.status.code()));
    }
    u64::try_from(output.stdout.len()).map_err(|_| "gzip output size exceeds u64".to_string())
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

/// Serve `web/dist` on `127.0.0.1:{port}` with standard COOP/COEP and cache headers.
fn serve_web(port: u16) -> u8 {
    let dist = PathBuf::from("web/dist");
    let index = dist.join("index.html");
    let wasm = dist.join("emath.wasm");
    if !index.is_file() || !wasm.is_file() {
        println!("serve-web: web/dist incomplete; building web assets...");
        let code = build_web();
        if code != 0 {
            return code;
        }
    }

    let addr = format!("127.0.0.1:{port}");
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("serve-web: failed to bind {addr}: {error}");
            return 1;
        }
    };

    println!("== serve-web ==");
    println!("Serving http://{addr}/");
    println!("Document root: {}", dist.display());
    println!("Headers: COOP=same-origin, COEP=require-corp");
    println!("Press Ctrl+C to stop");

    // One connection at a time on the accept thread. Unbounded
    // `thread::spawn` per accept detached JoinHandles and was a localhost
    // resource/DoS footgun (same fix as `emath web` / serve_cmd).
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = handle_http_connection(stream, &dist);
            }
            Err(error) => {
                eprintln!("serve-web: connection failed: {error}");
            }
        }
    }
    0
}

fn handle_http_connection(mut stream: std::net::TcpStream, dist_dir: &Path) -> std::io::Result<()> {
    use std::io::Read;

    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));

    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return Ok(()),
    };

    let request_str = String::from_utf8_lossy(&buf[..n]);
    let mut lines = request_str.lines();
    let req_line = match lines.next() {
        Some(l) => l,
        None => {
            return send_http_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"Bad Request",
                false,
                None,
            )
        }
    };

    let mut parts = req_line.split_whitespace();
    let method = match parts.next() {
        Some(m) => m,
        None => {
            return send_http_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"Bad Request",
                false,
                None,
            )
        }
    };
    let uri = match parts.next() {
        Some(u) => u,
        None => {
            return send_http_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"Bad Request",
                false,
                None,
            )
        }
    };

    if method != "GET" && method != "HEAD" {
        return send_http_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
            false,
            None,
        );
    }
    let is_head = method == "HEAD";

    let (path_part, query_part) = match uri.find('?') {
        Some(idx) => (&uri[..idx], Some(&uri[idx + 1..])),
        None => (uri, None),
    };

    let decoded_path = percent_decode(path_part);
    let clean_path = decoded_path.trim_start_matches('/');
    let target_rel = if clean_path.is_empty() {
        "index.html"
    } else {
        clean_path
    };

    if target_rel.contains("..") || target_rel.starts_with('/') || target_rel.starts_with('\\') {
        return send_http_response(
            &mut stream,
            403,
            "Forbidden",
            "text/plain; charset=utf-8",
            b"Forbidden",
            is_head,
            None,
        );
    }

    let file_path = dist_dir.join(target_rel);
    if !file_path.is_file() {
        return send_http_response(
            &mut stream,
            404,
            "Not Found",
            "text/html; charset=utf-8",
            b"<!DOCTYPE html><html><body><h1>404 Not Found</h1></body></html>",
            is_head,
            None,
        );
    }

    let file_bytes = match std::fs::read(&file_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let msg = format!("500 Internal Server Error: {error}");
            return send_http_response(
                &mut stream,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                msg.as_bytes(),
                is_head,
                None,
            );
        }
    };

    let mime = mime_for_path(&file_path);
    let cache_control = if target_rel == "index.html" || target_rel == "index.htm" {
        "no-cache, no-store, must-revalidate"
    } else if query_part.is_some_and(|q| q.starts_with("v="))
        || file_path.extension().and_then(|s| s.to_str()) == Some("wasm")
    {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    send_http_response(
        &mut stream,
        200,
        "OK",
        mime,
        &file_bytes,
        is_head,
        Some(cache_control),
    )
}

fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(v1), Some(v2)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((v1 << 4) | v2);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn send_http_response(
    stream: &mut std::net::TcpStream,
    status_code: u16,
    status_msg: &str,
    content_type: &str,
    body: &[u8],
    is_head: bool,
    cache_control: Option<&str>,
) -> std::io::Result<()> {
    use std::io::Write;
    let cc = cache_control.unwrap_or("no-cache");
    let header = format!(
        "HTTP/1.1 {status_code} {status_msg}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\n\
         Cross-Origin-Embedder-Policy: require-corp\r\n\
         Cache-Control: {cc}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if !is_head {
        stream.write_all(body)?;
    }
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_for_path() {
        assert_eq!(mime_for_path(Path::new("index.html")), "text/html; charset=utf-8");
        assert_eq!(mime_for_path(Path::new("app.js")), "text/javascript; charset=utf-8");
        assert_eq!(mime_for_path(Path::new("style.css")), "text/css; charset=utf-8");
        assert_eq!(mime_for_path(Path::new("emath.wasm")), "application/wasm");
        assert_eq!(mime_for_path(Path::new("data.json")), "application/json; charset=utf-8");
        assert_eq!(mime_for_path(Path::new("icon.png")), "image/png");
        assert_eq!(mime_for_path(Path::new("vector.svg")), "image/svg+xml");
        assert_eq!(mime_for_path(Path::new("unknown.bin")), "application/octet-stream");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("index.html"), "index.html");
        assert_eq!(percent_decode("%2Fpath%2Fto%2Ffile"), "/path/to/file");
        assert_eq!(percent_decode("invalid%2"), "invalid%2");
    }
}

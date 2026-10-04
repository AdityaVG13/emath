//! `emath compare`: evaluate one function under several carrier worlds
//! and report every answer side by side.
//!
//! A world is a configuration of the reference engine (see
//! `constructor_layer::world`): `reference` as authored, `exact` with
//! every real an exact rational, `float64` / `float32` / `bfloat16` with
//! real data rounded to that float format, `int64` with integer
//! arithmetic confined to signed 64-bit, `interval` with every real an
//! outward-rounded binary64 enclosure (an enclosure agrees with the
//! answers it contains), `affine` with every real exact and each range
//! input an affine form, so correlated quantities cancel.
//! The command never picks a winner. It prints each world's answer or
//! refusal, the exact gap of each numeric answer from the first answer,
//! and a verdict: `agree` (every world answered the same value),
//! `disagree` (answers differ), or `partial` (some worlds refused and the
//! answers that exist agree). Disagreement is a finding, not a failure,
//! so the exit status is 0 whenever every world was evaluated.

use std::collections::BTreeMap;
use std::path::PathBuf;

use emath_artifact::JsonWriter;
use emath_core::tree::{Item, StmtKind, TypeExpr};
use emath_exec_ir::constructor_layer::{
    CValue, CarrierWorld, DEFAULT_WORK, answer_gap, answers_agree, evaluate_function_carrying,
    exact_rat, interval_width, prepare_module, with_world, world_input,
};
use emath_exec_ir::exact_int::ExactInt;

use crate::{CliExit, EXIT_ADMISSION, EXIT_IO, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};

pub(crate) const USAGE: &str = "compare <file.emath> --function F [--set name=value] [--grid name=v1,v2,... | name=LO..HI | name=LO..HI/N] [--world reference|exact|float64|float32|bfloat16|int64|interval|affine] [--work N] [--json]";

/// Most input points one grid may evaluate.
const GRID_LIMIT: usize = 10_000;

#[derive(Clone, Debug)]
pub(crate) struct CompareRequest {
    pub path: PathBuf,
    pub function: String,
    pub given: Vec<(String, String)>,
    pub grid: Vec<(String, String)>,
    pub worlds: Vec<String>,
    pub work: u64,
    pub json: bool,
}

impl CompareRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let (mut path, mut function) = (None, None);
        let mut given = Vec::new();
        let mut grid = Vec::new();
        let mut worlds = Vec::new();
        let mut work = DEFAULT_WORK;
        let mut json = false;
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--function" => {
                    i += 1;
                    function = Some(rest.get(i)?.clone());
                }
                "--set" => {
                    i += 1;
                    let (k, v) = rest.get(i)?.split_once('=')?;
                    given.push((k.to_string(), v.to_string()));
                }
                "--grid" => {
                    i += 1;
                    let (k, v) = rest.get(i)?.split_once('=')?;
                    grid.push((k.to_string(), v.to_string()));
                }
                "--world" => {
                    i += 1;
                    worlds.push(rest.get(i)?.clone());
                }
                "--work" => {
                    i += 1;
                    work = rest.get(i)?.parse().ok()?;
                }
                "--json" => json = true,
                other => {
                    if other.starts_with('-') || path.is_some() {
                        return None;
                    }
                    path = Some(PathBuf::from(other));
                }
            }
            i += 1;
        }
        Some(Self {
            path: path?,
            function: function?,
            given,
            grid,
            worlds,
            work,
            json,
        })
    }
}

struct Outcome {
    world: CarrierWorld,
    inputs: BTreeMap<String, CValue>,
    answer: Result<CValue, (String, String)>,
}

/// What every returned answer asserts, in the form an independent
/// checker consumes: this function of this source, under this language
/// image and world, applied to these inputs (as the world read them),
/// returned this value. emath does not check it; `status` stays
/// `unchecked` until a checker answers for `id`.
struct Claim {
    id: String,
    statement: String,
}

struct Identity {
    language_id: String,
    source_id: String,
}

fn claim(identity: &Identity, function: &str, outcome: &Outcome, answer: &CValue) -> Claim {
    let inputs: Vec<String> = outcome
        .inputs
        .iter()
        .map(|(name, value)| format!("{name} = {value}"))
        .collect();
    let statement = format!(
        "{function}({}) = {answer} in world {}",
        inputs.join(", "),
        outcome.world.name()
    );
    // Canonical text: one field per line, inputs in name order (the map
    // is ordered), so the id is reproducible across runs and hosts.
    let canonical = format!(
        "emath.claim.v1\nlanguage={}\nsource={}\nfunction={function}\ninputs={}\nworld={}\nanswer={answer}\n",
        identity.language_id,
        identity.source_id,
        inputs.join("; "),
        outcome.world.name(),
    );
    Claim {
        id: emath_core::content_id_of_str(&canonical).0,
        statement,
    }
}

fn refuse(json: bool, code: &str, message: &str, exit: CliExit) -> CliExit {
    if json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.compare.v1");
        out.string("command", "compare");
        out.string("code", code);
        out.string("message", message);
        println!("{}", out.finish());
    } else {
        eprintln!("error {code}: {message}");
    }
    exit
}

/// Declared input types of the named entry in the main file.
fn declared_inputs(
    tree: &emath_core::tree::SyntaxTree,
    function: &str,
) -> BTreeMap<String, TypeExpr> {
    let mut types = BTreeMap::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.name != function {
            continue;
        }
        for section in decl.sections().filter(|s| s.name == "inputs") {
            for stmt in &section.suite.statements {
                if let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                    types.insert(name.clone(), ty.clone());
                }
            }
        }
    }
    types
}

fn representation(value: &CValue) -> &'static str {
    match value {
        CValue::Int(_) | CValue::Rat { .. } | CValue::Bool(_) => "exact_scalar",
        CValue::Float64(_) => "rounded_scalar",
        CValue::Interval { .. } => "enclosure",
        CValue::Affine(_) => "affine",
        CValue::Code(_) => "code",
        _ => "structured",
    }
}

pub(crate) fn run(request: CompareRequest) -> CliExit {
    let mut worlds = Vec::new();
    for name in &request.worlds {
        match CarrierWorld::parse(name) {
            Some(world) if !worlds.contains(&world) => worlds.push(world),
            Some(_) => {}
            None => {
                return refuse(
                    request.json,
                    "E-CLI-USAGE",
                    &format!(
                        "unknown world `{name}`; worlds are {}",
                        CarrierWorld::ALL.map(CarrierWorld::name).join(", ")
                    ),
                    EXIT_USAGE,
                );
            }
        }
    }
    if worlds.is_empty() {
        worlds = CarrierWorld::ALL.to_vec();
    }
    let source = match std::fs::read_to_string(&request.path) {
        Ok(text) => text,
        Err(err) => {
            return refuse(
                request.json,
                "E-CLI-IO",
                &format!("cannot read {}: {err}", request.path.display()),
                EXIT_IO,
            );
        }
    };
    let language_id = match crate::cli_dispatch::load_verified_language(Some(&request.path)) {
        Ok(distribution) => distribution.image.distribution_hash.to_string(),
        Err(detail) => return refuse(request.json, "E-LANG-IMAGE", &detail, EXIT_REFUSED),
    };
    let identity = Identity {
        language_id,
        source_id: emath_core::content_id_of_str(&source).0,
    };
    let tree = match crate::cli_json::admitted_tree(&request.path, &source) {
        Ok(tree) => tree,
        Err(refusal) => {
            if request.json {
                return refuse(true, &refusal.code, &refusal.message, EXIT_ADMISSION);
            }
            eprintln!("{}", refusal.text);
            return EXIT_ADMISSION;
        }
    };
    let declared = declared_inputs(&tree, &request.function);
    let points = match grid_points(&request) {
        Ok(points) => points,
        Err(message) => return refuse(request.json, "E-CLI-USAGE", &message, EXIT_USAGE),
    };
    let mut per_point: Vec<Vec<Outcome>> = points.iter().map(|_| Vec::new()).collect();
    for &world in &worlds {
        with_world(world, || {
            let module = prepare_module(&tree, Some(&request.path));
            for (point, outcomes) in points.iter().zip(&mut per_point) {
                let inputs: Result<BTreeMap<String, CValue>, _> = point
                    .iter()
                    .map(|(name, raw)| {
                        world_input(world, declared.get(name), raw)
                            .map(|value| (name.clone(), value))
                    })
                    .collect();
                let (inputs, answer) = match (inputs, &module) {
                    (Ok(inputs), Ok(module)) => {
                        let answer = evaluate_function_carrying(
                            module,
                            &request.function,
                            &inputs,
                            &mut BTreeMap::new(),
                            request.work,
                        )
                        .and_then(|run| run.value);
                        (inputs, answer)
                    }
                    (Err(err), _) => (BTreeMap::new(), Err(err)),
                    (Ok(_), Err(err)) => (BTreeMap::new(), Err(err.clone())),
                };
                outcomes.push(Outcome {
                    world,
                    inputs,
                    answer: answer.map_err(|err| (err.code, err.message)),
                });
            }
        });
    }
    if request.grid.is_empty() {
        report(&request, &identity, &per_point[0])
    } else {
        report_grid(&request, &identity, &worlds, &points, &per_point)
    }
}

/// The input assignments to evaluate: the `--set` inputs alone, or one
/// point per element of the product of the `--grid` axes (the last axis
/// varies fastest), each a full assignment in input text.
fn grid_points(request: &CompareRequest) -> Result<Vec<BTreeMap<String, String>>, String> {
    let fixed: BTreeMap<String, String> = request.given.iter().cloned().collect();
    let mut points = vec![fixed];
    for (name, spec) in &request.grid {
        let values = axis_values(spec).map_err(|why| format!("--grid {name}={spec}: {why}"))?;
        if points.len().saturating_mul(values.len()) > GRID_LIMIT {
            return Err(format!("the grid has more than {GRID_LIMIT} points"));
        }
        points = points
            .iter()
            .flat_map(|point| {
                values.iter().map(move |value| {
                    let mut point = point.clone();
                    point.insert(name.clone(), value.clone());
                    point
                })
            })
            .collect();
    }
    Ok(points)
}

/// `v1,v2,...` (as written), `LO..HI` (every integer), or `LO..HI/N`
/// (N + 1 evenly spaced exact points from LO to HI), as input text every
/// world reads in its own way.
fn axis_values(spec: &str) -> Result<Vec<String>, String> {
    if !spec.contains("..") {
        let values: Vec<String> = spec.split(',').map(|v| v.trim().to_string()).collect();
        if values.iter().any(String::is_empty) {
            return Err("expected v1,v2,... or LO..HI[/N]".to_string());
        }
        if values.len() > GRID_LIMIT {
            return Err(format!("the axis has more than {GRID_LIMIT} points"));
        }
        return Ok(values);
    }
    // The last `/` splits off the step count; `HI` may be a rational.
    let (range, steps) = match spec.rsplit_once('/') {
        Some((range, steps)) if range.contains("..") && !steps.contains("..") => {
            let steps: u64 = steps
                .parse()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or("the step count after `/` must be a positive integer")?;
            (range, Some(steps))
        }
        _ => (spec, None),
    };
    let (lo, hi) = range.split_once("..").ok_or("expected LO..HI")?;
    let exact = |text: &str| match world_input(CarrierWorld::Exact, None, text.trim()) {
        Ok(CValue::Int(n)) => Ok((n, ExactInt::one())),
        Ok(CValue::Rat { num, den }) => Ok((num, den)),
        _ => Err(format!("`{text}` is not a number")),
    };
    let ((ln, ld), (hn, hd)) = (exact(lo)?, exact(hi)?);
    let Some(steps) = steps else {
        let (Some(lo), Some(hi)) = (ln.to_i64(), hn.to_i64()) else {
            return Err("integer bounds must fit 64 bits".to_string());
        };
        if !ld.is_one() || !hd.is_one() {
            return Err("an axis without /N runs over integers".to_string());
        }
        if hi < lo {
            return Err("LO is above HI".to_string());
        }
        if hi.abs_diff(lo) >= GRID_LIMIT as u64 {
            return Err(format!("the axis has more than {GRID_LIMIT} points"));
        }
        return Ok((lo..=hi).map(|n| n.to_string()).collect());
    };
    if steps >= GRID_LIMIT as u64 {
        return Err(format!("the axis has more than {GRID_LIMIT} points"));
    }
    let n = ExactInt::from(i128::from(steps));
    let overflow = |_| "a grid point overflowed".to_string();
    (0..=steps)
        .map(|k| {
            // lo + (hi - lo) k / n = (lo (n - k) + hi k) / n, exactly.
            let k = ExactInt::from(i128::from(k));
            let low = ln
                .mul(&hd)
                .and_then(|a| a.mul(&n.sub(&k)?))
                .map_err(overflow)?;
            let high = hn.mul(&ld).and_then(|b| b.mul(&k)).map_err(overflow)?;
            let num = low.add(&high).map_err(overflow)?;
            let den = ld.mul(&hd).and_then(|d| d.mul(&n)).map_err(overflow)?;
            match exact_rat(num, den).map_err(|err| err.message)? {
                CValue::Rat { num, den } => Ok(exact_text(&num, &den)),
                other => Ok(other.to_string()),
            }
        })
        .collect()
}

/// `num/den` as a decimal when it terminates, else as `num/den`.
fn exact_text(num: &ExactInt, den: &ExactInt) -> String {
    if den.is_one() {
        return num.to_string();
    }
    let strip = |mut d: ExactInt, p: i128| {
        let p = ExactInt::from(p);
        let mut count = 0_usize;
        while d.rem_euclid(&p).is_ok_and(|r| r.is_zero()) {
            d = d.quot(&p).expect("p divides d");
            count += 1;
        }
        (d, count)
    };
    let (rest, twos) = strip(den.clone(), 2);
    let (rest, fives) = strip(rest, 5);
    if !rest.is_one() {
        return format!("{num}/{den}");
    }
    let places = twos.max(fives);
    let scaled = (0..places)
        .try_fold(num.clone(), |m, _| m.mul(&ExactInt::from(10_i128)))
        .and_then(|m| m.quot(den));
    let Ok(scaled) = scaled else {
        return format!("{num}/{den}");
    };
    let text = scaled.to_string();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", text.as_str()),
    };
    let digits = format!("{digits:0>width$}", width = places + 1);
    let (whole, frac) = digits.split_at(digits.len() - places);
    format!("{sign}{whole}.{frac}")
}

/// The verdict over one point's outcomes, and the largest gap from the
/// first answer with the world that has it.
fn assess(outcomes: &[Outcome]) -> (&'static str, Option<(f64, CarrierWorld)>) {
    let baseline = outcomes.iter().find_map(|o| o.answer.as_ref().ok());
    let mut differ = false;
    let mut refused = false;
    let mut worst: Option<(f64, CarrierWorld)> = None;
    for outcome in outcomes {
        match &outcome.answer {
            Ok(value) => {
                let base = baseline.expect("a returned answer implies a baseline");
                // A comparison that cannot be made (a buffer) counts as a
                // difference: the worlds were not shown to agree.
                if !answers_agree(value, base).unwrap_or(false) {
                    differ = true;
                }
                if let Some(gap) = answer_gap(value, base).map(f64::abs) {
                    if gap > 0.0 && worst.is_none_or(|(g, _)| gap > g) {
                        worst = Some((gap, outcome.world));
                    }
                }
            }
            Err(_) => refused = true,
        }
    }
    let verdict = match (differ, refused) {
        (true, _) => "disagree",
        (false, true) => "partial",
        (false, false) => "agree",
    };
    (verdict, worst)
}

fn point_text(point: &BTreeMap<String, String>) -> String {
    point
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A divergence map: every grid point in every world, reported as
/// verdict counts, the points where the worlds do not agree, and the
/// point with the largest gap.
fn report_grid(
    request: &CompareRequest,
    identity: &Identity,
    worlds: &[CarrierWorld],
    points: &[BTreeMap<String, String>],
    per_point: &[Vec<Outcome>],
) -> CliExit {
    let mut counts = BTreeMap::from([("agree", 0_u64), ("disagree", 0), ("partial", 0)]);
    let mut divergent = Vec::new();
    let mut worst: Option<(f64, CarrierWorld, usize)> = None;
    for (index, outcomes) in per_point.iter().enumerate() {
        let (verdict, gap) = assess(outcomes);
        *counts.get_mut(verdict).expect("known verdict") += 1;
        if let Some((gap, world)) = gap {
            if worst.is_none_or(|(g, _, _)| gap > g) {
                worst = Some((gap, world, index));
            }
        }
        if verdict != "agree" {
            divergent.push((index, verdict, gap));
        }
    }
    let point_row = |index: usize, verdict: &str, gap: Option<(f64, CarrierWorld)>| {
        let mut inputs = JsonWriter::object();
        for (name, value) in &points[index] {
            inputs.string(name, value);
        }
        let mut row = JsonWriter::object();
        row.object_field("inputs", &inputs.finish());
        row.string("verdict", verdict);
        if let Some((gap, world)) = gap {
            row.field("max_gap", &format!("{gap:e}"));
            row.string("max_gap_world", world.name());
        }
        let mut refused = JsonWriter::object();
        for outcome in &per_point[index] {
            if let Err((code, _)) = &outcome.answer {
                refused.string(outcome.world.name(), code);
            }
        }
        row.object_field("refused", &refused.finish());
        row.finish()
    };
    if request.json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.compare.grid.v1");
        out.string("command", "compare");
        out.string("function", &request.function);
        out.string("language_id", &identity.language_id);
        out.string("source_id", &identity.source_id);
        let names: Vec<String> = worlds.iter().map(|w| w.name().to_string()).collect();
        out.strings("worlds", &names);
        out.int("points", points.len() as u64);
        for (verdict, count) in &counts {
            out.int(verdict, *count);
        }
        let rows: Vec<String> = divergent
            .iter()
            .map(|&(index, verdict, gap)| point_row(index, verdict, gap))
            .collect();
        out.objects("divergent", &rows);
        if let Some((gap, world, index)) = worst {
            let (verdict, _) = assess(&per_point[index]);
            out.object_field("worst", &point_row(index, verdict, Some((gap, world))));
        }
        println!("{}", out.finish());
    } else {
        const SHOWN: usize = 20;
        println!(
            "grid: {} points; agree {}, disagree {}, partial {}",
            points.len(),
            counts["agree"],
            counts["disagree"],
            counts["partial"]
        );
        for &(index, verdict, gap) in divergent.iter().take(SHOWN) {
            let mut line = format!("{verdict:<9} {}", point_text(&points[index]));
            if let Some((gap, world)) = gap {
                line.push_str(&format!("   max gap {gap:e} ({})", world.name()));
            }
            for outcome in &per_point[index] {
                if let Err((code, _)) = &outcome.answer {
                    line.push_str(&format!("   {} refused {code}", outcome.world.name()));
                }
            }
            println!("{line}");
        }
        if divergent.len() > SHOWN {
            println!(
                "... {} more (--json lists every point)",
                divergent.len() - SHOWN
            );
        }
        if let Some((gap, world, index)) = worst {
            println!(
                "worst: {}   gap {gap:e} ({})",
                point_text(&points[index]),
                world.name()
            );
        }
    }
    EXIT_OK
}

fn report(request: &CompareRequest, identity: &Identity, outcomes: &[Outcome]) -> CliExit {
    let baseline = outcomes.iter().find_map(|o| o.answer.as_ref().ok());
    let mut rows = Vec::new();
    let mut lines = Vec::new();
    for outcome in outcomes {
        let mut row = JsonWriter::object();
        row.string("world", outcome.world.name());
        match &outcome.answer {
            Ok(value) => {
                let base = baseline.expect("a returned answer implies a baseline");
                let gap = answer_gap(value, base);
                row.string("execution", "returned");
                row.string("representation", representation(value));
                row.string("payload", &value.to_string());
                let mut line = format!("{:<10} {}", outcome.world.name(), value);
                if let Some(gap) = gap.filter(|g| *g != 0.0) {
                    row.field("gap", &format!("{gap:e}"));
                    line.push_str(&format!("   (gap {gap:e})"));
                }
                if let Some(width) = interval_width(value) {
                    row.field("width", &format!("{width:e}"));
                    line.push_str(&format!("   (width {width:e})"));
                }
                let claim = claim(identity, &request.function, outcome, value);
                let mut claim_row = JsonWriter::object();
                claim_row.string("id", &claim.id);
                claim_row.string("statement", &claim.statement);
                claim_row.string("status", "unchecked");
                row.object_field("claim", &claim_row.finish());
                lines.push(line);
            }
            Err((code, message)) => {
                row.string("execution", "refused");
                row.string("code", code);
                row.string("message", message);
                lines.push(format!(
                    "{:<10} refused {code}: {message}",
                    outcome.world.name()
                ));
            }
        }
        rows.push(row.finish());
    }
    let (verdict, _) = assess(outcomes);
    if request.json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.compare.v1");
        out.string("command", "compare");
        out.string("function", &request.function);
        out.string("language_id", &identity.language_id);
        out.string("source_id", &identity.source_id);
        out.objects("worlds", &rows);
        out.string("verdict", verdict);
        println!("{}", out.finish());
    } else {
        lines.iter().for_each(|line| println!("{line}"));
        println!("verdict: {verdict}");
    }
    EXIT_OK
}

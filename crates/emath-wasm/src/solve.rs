//! Solve-world dispositions and candidate listing.

use super::*;

pub(super) fn solve_disposition(source: &str, world: emath_syntax::SolveWorld) -> Disposition {
    let normalized: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();
    if !normalized.starts_with("solvex^2=2") {
        return Disposition::Refused {
            reason: "intent candidate execution currently requires the admitted equation `x^2 = 2`"
                .to_string(),
        };
    }
    match world {
        emath_syntax::SolveWorld::RealPm => Disposition::Answer {
            canonical: "x ∈ {-sqrt(2), sqrt(2)} over Real".to_string(),
        },
        emath_syntax::SolveWorld::Complex => Disposition::Answer {
            canonical: "x ∈ {-sqrt(2) + 0i, sqrt(2) + 0i} over Complex".to_string(),
        },
        emath_syntax::SolveWorld::Symbolic => Disposition::Answer {
            canonical: "roots(x^2 - 2) = {-sqrt(2), sqrt(2)}".to_string(),
        },
        emath_syntax::SolveWorld::Modular => Disposition::Open {
            missing: vec!["modulus".to_string()],
        },
        emath_syntax::SolveWorld::Numeric => Disposition::Open {
            missing: vec!["tolerance".to_string()],
        },
    }
}

pub(super) fn solve_world_result(source: &str, world: emath_syntax::SolveWorld) -> WorldResult {
    WorldResult {
        world: world.as_str().to_string(),
        origin: "intent-completion".to_string(),
        method: world.method().to_string(),
        term_canonical: "solve x^2 = 2".to_string(),
        inputs: BTreeMap::new(),
        assumptions: vec![
            format!("domain={}", world.domain()),
            format!("exactness={}", world.exactness()),
        ],
        disposition: solve_disposition(source, world),
        evidence_laws: vec![format!("{} verification", world.evidence_class())],
        cost_steps: 0,
    }
}

pub(super) fn op_solve_candidates(payload: &str) -> String {
    let request = match parse_solve_payload(payload) {
        Ok(request) => request,
        Err(message) => return error_json(&message),
    };
    let expansion = emath_syntax::expand_scratch(&request.source);
    if expansion.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &expansion.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&expansion.diagnostics));
        return object.finish();
    }
    if matches!(expansion.solve, emath_syntax::SolveIntent::Absent) {
        return error_json("source has no `solve` intent");
    }

    let worlds: Vec<emath_syntax::SolveWorld> = request
        .apply
        .map_or_else(|| expansion.solve.menu().to_vec(), |world| vec![world]);
    let results = worlds
        .iter()
        .map(|world| solve_world_result(&request.source, *world))
        .collect();
    let Ok(bundle) = ResultBundle::new(results) else {
        return error_json("solve candidate produced a naked result");
    };

    let mut teacher = JsonWriter::object();
    teacher.string("understood", "solve x^2 = 2");
    teacher.string("missing", "domain, numeric policy, and method selection");
    teacher.string(
        "repair",
        "choose a labeled candidate; candidates with parameters retain typed holes",
    );
    teacher.string(
        "authority",
        "no candidate is promoted to the intended meaning until selected",
    );

    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.strings(
        "ambiguity_dimensions",
        &[
            "domain".to_string(),
            "numeric".to_string(),
            "method".to_string(),
        ],
    );
    object.object_field("teacher", &teacher.finish());
    object.object_field("result_bundle", &bundle.to_json());
    if let Some(world) = request.apply {
        match emath_syntax::apply_solve_candidate(&request.source, world) {
            Ok((rewritten, delta)) => {
                object.string("apply", world.as_str());
                object.string("source", &rewritten);
                object.string("meaning_delta", &delta);
            }
            Err(message) => return error_json(&message),
        }
    }
    object.finish()
}

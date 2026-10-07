use super::{CliExit, EXIT_REFUSED, EXIT_USAGE, RunRequest, constructor_step, diagnostic, run_constructor_layer};

pub(crate) fn run(request: RunRequest) -> CliExit {
    let path = match std::fs::canonicalize(&request.path) {
        Ok(path) => path,
        Err(error) => return diagnostic(request.json, EXIT_USAGE, "E-PKG-080", &error.to_string()),
    };
    if let Err(detail) = crate::cli_dispatch::load_verified_language(Some(&path)) {
        return diagnostic(request.json, EXIT_REFUSED, "E-LANG-IMAGE", &detail);
    }
    let checked = crate::cli_build::checked_file(&path);
    let mut worlds = Vec::new();
    for name in &request.worlds {
        match emath_exec_ir::constructor_layer::CarrierWorld::parse(name) {
            Some(world) => worlds.push(world),
            None => {
                let known: Vec<&str> =
                    emath_exec_ir::constructor_layer::CarrierWorld::ALL.iter().map(|w| w.name()).collect();
                return diagnostic(
                    request.json,
                    EXIT_USAGE,
                    "unknown_world",
                    &format!("unknown world `{name}` in --worlds; worlds are {}", known.join(", ")),
                );
            }
        }
    }
    if worlds.is_empty() {
        run_constructor_layer(&request, &checked)
    } else {
        emath_exec_ir::constructor_layer::with_worlds(&worlds, || run_constructor_layer(&request, &checked))
    }
}

pub(crate) fn step(request: RunRequest) -> CliExit {
    constructor_step(&request).unwrap_or_else(|| not_a_checkpoint(&request.path, request.json))
}

pub(super) fn not_a_checkpoint(path: &std::path::Path, json: bool) -> CliExit {
    diagnostic(
        json,
        crate::EXIT_CHECKPOINT,
        "incompatible_checkpoint",
        &format!(
            "{} is not a readable constructor checkpoint; `emath run --work N` writes one",
            path.display()
        ),
    )
}

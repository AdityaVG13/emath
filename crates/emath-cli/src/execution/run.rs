use super::*;

#[allow(unreachable_code, unused_variables)]
pub(crate) fn run(request: RunRequest) -> CliExit {
    if let Some(exit) = crate::refuse_malformed_project_lock(&request.path) {
        return diagnostic(
            request.json,
            exit,
            "E-RUN-LOCK",
            "the project meaning lock refused this source",
        );
    }
    let path = match std::fs::canonicalize(&request.path) {
        Ok(path) => path,
        Err(error) => return diagnostic(request.json, EXIT_USAGE, "E-PKG-080", &error.to_string()),
    };
    let language_id = match installed_language(&path, request.json) {
        Ok(id) => id,
        Err(exit) => return exit,
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => return diagnostic(request.json, EXIT_USAGE, "E-PKG-080", &error.to_string()),
    };
    if let Some(exit) = run_constructor_layer(&request, &source) {
        return exit;
    }
    return diagnostic(
        request.json,
        EXIT_ADMISSION,
        "E-KIND-GONE",
        "`emath run` evaluates `emath object`, `emath function`, and `emath query`. Other declaration kinds are not constructors.",
    );
    let package = match planned_package(&path, request.json) {
        Ok(package) => package,
        Err(exit) => return exit,
    };
    let meaning_id = match package.meaning_id(&[]) {
        Ok(id) => id.to_string(),
        Err(error) => {
            return diagnostic(
                request.json,
                EXIT_REFUSED,
                "E-RUN-IDENTITY",
                &format!("{error:?}"),
            );
        }
    };
    let mut state = SavedRun {
        source_path: path,
        source,
        meaning_id,
        language_id,
        function: request.function.clone(),
        given: request.given.clone(),
        completed: Vec::new(),
        active_result: None,
        methods: BTreeMap::new(),
        revision: 0,
        total: 0,
        measure: request.measure,
        measurements: Vec::new(),
        branch: None,
    };
    // Typed-hole discipline (E-GOAL-043): the runner executes evaluate
    // cases. A no-test declaration carrying a goal outside that
    // executable subset (a scratch `find` lowers to `search` — an open
    // hole stays symbolic) refuses before any checkpoint is written:
    // never a produced crate for an unexecutable goal. Declarations with
    // tests run their pinned cases; the goals do not gate those.
    for declaration in &package.declarations {
        if state
            .function
            .as_deref()
            .is_some_and(|name| declaration.name.leaf() != name)
        {
            continue;
        }
        if declaration.tests.is_empty() {
            for goal_id in &declaration.goals {
                if let Some(goal) = package.goal(*goal_id)
                    && goal.kind != GoalKind::Evaluate
                {
                    return diagnostic(
                        request.json,
                        EXIT_REFUSED,
                        "E-GOAL-043",
                        &format!(
                            "goal kind `{}` on `{}` is not a constructor; write an ordinary `emath function` or `emath query` and `emath run`",
                            goal.kind.as_str(),
                            declaration.name.leaf(),
                        ),
                    );
                }
            }
        }
    }
    if let (Some(parent), Some(relation)) = (&request.branch_from, &request.relation) {
        let ancestry = std::fs::canonicalize(parent)
            .map_err(|error| error.to_string())
            .and_then(|path| SavedRun::load_at(&path, 1).map(|parent| (path, parent)))
            .and_then(|(path, parent)| state.attach_branch(path, &parent, relation));
        if let Err(error) = ancestry {
            return diagnostic(request.json, EXIT_REFUSED, "E-RUN-RELATION", &error);
        }
    }
    advance(&mut state, &package, &request, None)
}

pub(crate) fn step(request: RunRequest) -> CliExit {
    if let Some(exit) = constructor_step(&request) {
        return exit;
    }
    let mut state = match SavedRun::load(&request.path) {
        Ok(state) => state,
        Err(error) => return diagnostic(request.json, EXIT_USAGE, "E-RUN-STATE", &error),
    };
    if request
        .expected_revision
        .is_some_and(|revision| revision != state.revision)
    {
        return diagnostic(
            request.json,
            EXIT_REFUSED,
            "E-RUN-REVISION",
            "the saved revision does not match --expect-revision",
        );
    }
    let package = match resume_package(&state, request.json) {
        Ok(package) => package,
        Err(exit) => return exit,
    };
    if state.completed.len() == state.total {
        return emit(&state, &request.path, request.json, None);
    }
    advance(&mut state, &package, &request, Some(&request.path))
}

pub(super) fn resume_package(state: &SavedRun, json: bool) -> Result<SemanticPackage, CliExit> {
    if let Some(exit) = crate::refuse_malformed_project_lock(&state.source_path) {
        return Err(diagnostic(
            json,
            exit,
            "E-RUN-LOCK",
            "the project meaning lock refused this source",
        ));
    }
    let current = std::fs::read_to_string(&state.source_path)
        .map_err(|error| diagnostic(json, EXIT_USAGE, "E-PKG-080", &error.to_string()))?;
    if current != state.source {
        return Err(diagnostic(
            json,
            EXIT_REFUSED,
            "E-RUN-DRIFT",
            "source changed; start a new run instead of changing this checkpoint's target",
        ));
    }
    if installed_language(&state.source_path, json)? != state.language_id {
        return Err(diagnostic(
            json,
            EXIT_REFUSED,
            "E-RUN-DRIFT",
            "the Language Image changed; start a new run",
        ));
    }
    let package = checked_package(&state.source_path, json)?;
    if package
        .meaning_id(&[])
        .ok()
        .map(|id| id.to_string())
        .as_deref()
        != Some(&state.meaning_id)
    {
        return Err(diagnostic(
            json,
            EXIT_REFUSED,
            "E-RUN-DRIFT",
            "admitted meaning changed; start a new run",
        ));
    }
    Ok(package)
}

// Cases are independent calls. A checkpoint skips already committed cases;
// it never advertises suspension inside an opaque capability or solver.


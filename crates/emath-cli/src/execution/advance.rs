use super::*;

pub(super) fn jobs(
    package: &SemanticPackage,
    state: &SavedRun,
) -> Result<Vec<(usize, Option<usize>)>, String> {
    let selected: Vec<_> = package
        .declarations
        .iter()
        .enumerate()
        .filter(|(_, declaration)| {
            state
                .function
                .as_deref()
                .is_none_or(|name| declaration.name.leaf() == name)
        })
        .collect();
    if selected.is_empty() {
        return Err("no declaration matches --function".into());
    }
    if !state.given.is_empty() && selected.len() != 1 {
        return Err("use --function when --set targets a file with several declarations".into());
    }
    let mut jobs = Vec::new();
    for (index, declaration) in selected {
        if state.given.is_empty() && !declaration.tests.is_empty() {
            jobs.extend(
                declaration
                    .tests
                    .iter()
                    .map(|test| (index, Some(test.index()))),
            );
        } else {
            jobs.push((index, None));
        }
    }
    Ok(jobs)
}

pub(super) fn advance(
    state: &mut SavedRun,
    package: &SemanticPackage,
    request: &RunRequest,
    origin: Option<&Path>,
) -> CliExit {
    let jobs = match jobs(package, state) {
        Ok(jobs) => jobs,
        Err(error) => return diagnostic(request.json, EXIT_REFUSED, "E-EVAL-002", &error),
    };
    if state.total != 0 && state.total != jobs.len() {
        return diagnostic(
            request.json,
            EXIT_REFUSED,
            "E-RUN-STATE",
            "the saved case plan no longer matches the source",
        );
    }
    state.total = jobs.len();
    let directory = match std::fs::create_dir_all(&request.out)
        .and_then(|()| std::fs::canonicalize(&request.out))
    {
        Ok(directory) => directory,
        Err(error) => {
            return diagnostic(request.json, EXIT_USAGE, "E-RUN-STATE", &error.to_string());
        }
    };
    let mut path = match origin {
        Some(path) => match std::fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) => {
                return diagnostic(request.json, EXIT_USAGE, "E-RUN-STATE", &error.to_string());
            }
        },
        None => {
            let path = checkpoint_path(state, &directory);
            if let Err(error) = save(state, &path) {
                return diagnostic(request.json, EXIT_USAGE, "E-RUN-STATE", &error);
            }
            path
        }
    };
    // The first durable checkpoint is discoverable even if this process
    // receives SIGKILL before it can print its final JSON response.
    eprintln!("checkpoint: {}", path.display());
    let mut claim = JsonWriter::object();
    claim.string(
        "request_id",
        &content_id_of_str(&format!("{}\nwork={}\n", state.payload(), request.work)).0,
    );
    claim.string("origin", &path.to_string_lossy());
    claim.int("revision", state.revision as u64);
    claim.int("work", request.work as u64);
    let claim = claim.finish();
    let until = state.revision.saturating_add(request.work);
    while state.revision < until && state.completed.len() < state.total {
        let lock = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))
        {
            Ok(lock) => lock,
            Err(error) => {
                return emit(
                    state,
                    &path,
                    request.json,
                    Some((EXIT_USAGE, "E-RUN-STATE", &error.to_string())),
                );
            }
        };
        if let Err(error) = lock.try_lock() {
            return emit(
                state,
                &path,
                request.json,
                Some((
                    EXIT_REFUSED,
                    "E-RUN-BUSY",
                    &format!(
                        "revision is in use; retry the same request after its writer exits: {error}"
                    ),
                )),
            );
        }
        let claim_path = path.with_extension("request.json");
        match std::fs::read_to_string(&claim_path) {
            Ok(saved) if saved != claim => {
                return emit(
                    state,
                    &path,
                    request.json,
                    Some((
                        EXIT_REFUSED,
                        "E-RUN-REVISION",
                        "another request owns this revision; inspect its checkpoint or retry its original work grant",
                    )),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return emit(
                    state,
                    &path,
                    request.json,
                    Some((EXIT_USAGE, "E-RUN-STATE", &error.to_string())),
                );
            }
        }
        match successor(state, &path) {
            Ok(Some((saved, destination))) => {
                *state = saved;
                path = destination;
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                return emit(
                    state,
                    &path,
                    request.json,
                    Some((EXIT_USAGE, "E-RUN-STATE", &error)),
                );
            }
        }
        if let Some(cancel) = &request.cancel_file {
            match cancel.try_exists() {
                Ok(true) => {
                    return emit(
                        state,
                        &path,
                        request.json,
                        Some((
                            EXIT_REFUSED,
                            "E-RUN-CANCELLED",
                            "cancel file exists; completed cases remain committed; cancellation does not interrupt a running capability",
                        )),
                    );
                }
                Ok(false) => {}
                Err(error) => {
                    return emit(
                        state,
                        &path,
                        request.json,
                        Some((EXIT_USAGE, "E-RUN-CANCEL", &error.to_string())),
                    );
                }
            }
        }
        if let Err(error) = save_document(&claim, &claim_path) {
            return emit(
                state,
                &path,
                request.json,
                Some((EXIT_USAGE, "E-RUN-STATE", &error)),
            );
        }
        let parent_id = content_id_of_str(&state.payload()).0;
        let (declaration, test) = jobs[state.completed.len()];
        let (result, measurement, frames) = match execute_measured_case(
            package,
            &package.declarations[declaration],
            test,
            &state.given,
            state.measure,
            state.methods.get(&state.completed.len()).map(Vec::as_slice).unwrap_or(&[]),
        ) {
            Ok(result) => result,
            Err(error) => {
                return emit(
                    state,
                    &path,
                    request.json,
                    Some((
                        EXIT_REFUSED,
                        if error.starts_with("E-MEASURE-RESULT:") {
                            "E-MEASURE-RESULT"
                        } else {
                            "E-EVAL-005"
                        },
                        &error,
                    )),
                );
            }
        };
        let case = state.completed.len();
        let faulted = frames.iter().any(|frame| frame.fault.is_some());
        let finished = !faulted && frames.iter().all(|frame| progress::complete(&frame.state) == Some(true));
        let previous_active = state.active_result.take();
        let previous_frames = if frames.is_empty() { state.methods.remove(&case) } else { state.methods.insert(case, frames) };
        if finished { state.completed.push(result); } else { state.active_result = Some(result); }
        state.revision += 1;
        if let Some(measurement) = measurement {
            state.measurements.push(measurement);
        }
        let destination = checkpoint_path(state, &directory);
        let committed = save(state, &destination).and_then(|()| {
            let mut next = JsonWriter::object();
            next.string("parent", &parent_id);
            next.string("checkpoint", &destination.to_string_lossy());
            save_document(&next.finish(), &path.with_extension("next.json"))
        });
        if let Err(error) = committed {
            state.revision -= 1;
            if finished { state.completed.pop(); }
            state.active_result = previous_active;
            if let Some(frames) = previous_frames { state.methods.insert(case, frames); } else { state.methods.remove(&case); }
            if state.measure > 0 {
                state.measurements.pop();
            }
            return emit(
                state,
                &path,
                request.json,
                Some((EXIT_USAGE, "E-RUN-STATE", &error)),
            );
        }
        path = destination;
        if faulted { break; }
        // Dropping the file releases the OS lock, including after a crash.
        // A competing request cannot publish a second successor for a revision.
    }
    emit(state, &path, request.json, None)
}

pub(super) fn checkpoint_path(state: &SavedRun, directory: &Path) -> PathBuf {
    directory.join(format!(
        "{}.json",
        content_id_of_str(&state.payload()).0.replace(':', "-")
    ))
}

pub(super) fn successor(state: &SavedRun, path: &Path) -> Result<Option<(SavedRun, PathBuf)>, String> {
    let bytes = match std::fs::read_to_string(path.with_extension("next.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let next = parse_json_document(&bytes).map_err(|error| error.to_string())?;
    if text(&next, "parent")? != content_id_of_str(&state.payload()).0 {
        return Err("successor refers to a different parent checkpoint".into());
    }
    let path = PathBuf::from(text(&next, "checkpoint")?);
    let saved = SavedRun::load(&path)?;
    if saved.target_id() != state.target_id()
        || saved.branch_json() != state.branch_json()
        || saved.total != state.total
        || saved.measure != state.measure
        || state.revision.checked_add(1) != Some(saved.revision)
        || saved.completed.len() < state.completed.len()
        || saved.completed.len() > state.completed.len() + 1
        || state.methods.iter().any(|(case, frames)| *case < state.completed.len() && saved.methods.get(case) != Some(frames))
        || !saved.completed.starts_with(&state.completed)
        || !saved.measurements.starts_with(&state.measurements)
    {
        return Err("successor changes the target, policy, or committed result prefix".into());
    }
    Ok(Some((saved, path)))
}


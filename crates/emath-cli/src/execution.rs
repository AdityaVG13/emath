//! Answer-first execution over the existing declaration runner.
//!
//! A work unit initializes one case or advances one authored method.
//! Each unit commits before the next starts. OS locks serialize revisions.
//! Checkpoints retain exact values and capsule-owned continuation contracts.
//! Source, Language Image, bindings and runner version stay fixed on resume.

use crate::{CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_artifact::{JsonValue, JsonWriter, parse_json_document};
use emath_core::{content_id_of_str, limits::Limits};
use emath_exec_ir::interp::Value;
use emath_exec_ir::progress::{self, MethodFrame};
use emath_exec_ir::runner::{TestRun, TestVerdict, run_direct, run_test};
use emath_ir::{GoalKind, SemanticPackage, TypeNode};
use emath_sema::CompilerSession;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const SCHEMA: &str = "emath.run.v1";
const CHECKPOINT_SCHEMA: &str = "emath.run-checkpoint.v1";
const ENGINE: &str = concat!("authored-methods/4;emath/", env!("CARGO_PKG_VERSION"));
const DEFAULT_WORK: usize = 1024;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct RunRequest {
    pub path: PathBuf,
    pub out: PathBuf,
    pub function: Option<String>,
    pub given: BTreeMap<String, String>,
    pub work: usize,
    pub expected_revision: Option<usize>,
    pub cancel_file: Option<PathBuf>,
    pub measure: usize,
    pub branch_from: Option<PathBuf>,
    pub relation: Option<String>,
    pub json: bool,
}

impl RunRequest {
    pub(crate) fn parse(args: &[String], continuation: bool) -> Option<Self> {
        let mut path = None;
        let mut out = None;
        let mut function = None;
        let mut given = BTreeMap::new();
        let mut work = None;
        let mut expected_revision = None;
        let mut cancel_file = None;
        let mut measure = None;
        let mut branch_from = None;
        let mut relation = None;
        let mut json = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--out" | "-o" => {
                    crate::assign_once(
                        &mut out,
                        PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                    )?;
                }
                "--function" if !continuation => {
                    crate::assign_once(
                        &mut function,
                        crate::take_nonflag_value(args, &mut index)?.to_string(),
                    )?;
                }
                "--set" if !continuation => {
                    let raw = crate::take_nonflag_value(args, &mut index)?;
                    let (name, value) = raw.split_once('=')?;
                    if name.is_empty()
                        || value.is_empty()
                        || given.insert(name.to_string(), value.to_string()).is_some()
                    {
                        return None;
                    }
                }
                "--work" => {
                    let value: usize = crate::take_nonflag_value(args, &mut index)?.parse().ok()?;
                    if value == 0 {
                        return None;
                    }
                    crate::assign_once(&mut work, value)?;
                }
                "--expect-revision" if continuation => {
                    crate::assign_once(
                        &mut expected_revision,
                        crate::take_nonflag_value(args, &mut index)?.parse().ok()?,
                    )?;
                }
                "--cancel-file" => {
                    crate::assign_once(
                        &mut cancel_file,
                        PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                    )?;
                }
                "--measure" if !continuation => {
                    let value: usize = crate::take_nonflag_value(args, &mut index)?.parse().ok()?;
                    if !(1..=32).contains(&value) {
                        return None;
                    }
                    crate::assign_once(&mut measure, value)?;
                }
                "--branch-from" if !continuation => {
                    crate::assign_once(
                        &mut branch_from,
                        PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                    )?;
                }
                "--relation" if !continuation => {
                    let value = crate::take_nonflag_value(args, &mut index)?;
                    if !valid_relation(value) {
                        return None;
                    }
                    crate::assign_once(&mut relation, value.to_string())?;
                }
                "--json" => json = true,
                flag if flag.starts_with('-') => return None,
                value => crate::assign_once(&mut path, PathBuf::from(value))?,
            }
            index += 1;
        }
        if branch_from.is_some() != relation.is_some() {
            return None;
        }
        let path = path?;
        let out = out.unwrap_or_else(|| {
            if continuation {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            } else {
                PathBuf::from("target/emath/runs")
            }
        });
        Some(Self {
            path,
            out,
            function,
            given,
            work: work.unwrap_or(DEFAULT_WORK),
            expected_revision,
            cancel_file,
            measure: measure.unwrap_or(0),
            branch_from,
            relation,
            json,
        })
    }
}

fn valid_relation(value: &str) -> bool {
    matches!(
        value,
        "same-problem"
            | "special-case"
            | "relaxation"
            | "different-objective"
            | "constructed-world"
    )
}

#[derive(Clone)]
struct Branch {
    parent: PathBuf,
    relation: String,
    // Derived from the validated parent chain; never loaded as authority.
    original_target: String,
    preserves_original: bool,
}

#[derive(Clone)]
struct SavedRun {
    source_path: PathBuf,
    source: String,
    meaning_id: String,
    language_id: String,
    function: Option<String>,
    given: BTreeMap<String, String>,
    // Opaque result documents preserve exact rendered values without
    // reparsing them through floating JSON numbers on continuation.
    completed: Vec<String>,
    active_result: Option<String>,
    methods: BTreeMap<usize, Vec<MethodFrame>>,
    revision: usize,
    total: usize,
    measure: usize,
    measurements: Vec<String>,
    branch: Option<Branch>,
}

impl SavedRun {
    fn target_id(&self) -> String {
        let mut out = JsonWriter::object();
        out.string("source_path", &self.source_path.to_string_lossy());
        out.string("source", &self.source);
        out.string("meaning_id", &self.meaning_id);
        out.string("language_id", &self.language_id);
        out.string("function", self.function.as_deref().unwrap_or(""));
        let mut given = JsonWriter::object();
        for (name, value) in &self.given {
            given.string(name, value);
        }
        out.object_field("given", &given.finish());
        content_id_of_str(&out.finish()).0
    }

    fn preserves_original(&self) -> bool {
        self.branch
            .as_ref()
            .is_none_or(|branch| branch.preserves_original)
    }

    fn original_target(&self) -> String {
        self.branch
            .as_ref()
            .map_or_else(|| self.target_id(), |branch| branch.original_target.clone())
    }

    fn branch_json(&self) -> String {
        let mut out = JsonWriter::object();
        if let Some(branch) = &self.branch {
            out.string("parent", &branch.parent.to_string_lossy());
            out.string("relation", &branch.relation);
        }
        out.finish()
    }

    fn payload(&self) -> String {
        let mut out = JsonWriter::object();
        out.string("engine", ENGINE);
        out.string("source_path", &self.source_path.to_string_lossy());
        out.string("source", &self.source);
        out.string("meaning_id", &self.meaning_id);
        out.string("language_id", &self.language_id);
        out.string("function", self.function.as_deref().unwrap_or(""));
        let mut given = JsonWriter::object();
        for (name, value) in &self.given {
            given.string(name, value);
        }
        out.object_field("given", &given.finish());
        out.strings("completed", &self.completed);
        out.string("active_result", self.active_result.as_deref().unwrap_or(""));
        out.int("revision", self.revision as u64);
        let mut methods = JsonWriter::object();
        for (case, frames) in &self.methods { methods.objects(&case.to_string(), &frames_json(frames)); }
        out.object_field("methods", &methods.finish());
        out.int("total", self.total as u64);
        out.int("measure", self.measure as u64);
        out.strings("measurements", &self.measurements);
        out.object_field("branch", &self.branch_json());
        out.finish()
    }

    fn load(path: &Path) -> Result<Self, String> {
        Self::load_at(path, 0)
    }

    fn load_at(path: &Path, depth: usize) -> Result<Self, String> {
        if depth >= 64 {
            return Err("branch ancestry exceeds 64 checkpoints or contains a cycle".into());
        }
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let envelope = parse_json_document(&source).map_err(|error| error.to_string())?;
        if text(&envelope, "schema")? != CHECKPOINT_SCHEMA {
            return Err("not an emath run checkpoint".into());
        }
        let payload = text(&envelope, "payload")?;
        if text(&envelope, "content_id")? != content_id_of_str(&payload).0 {
            return Err(
                "checkpoint content identity does not match; do not reuse its results".into(),
            );
        }
        let doc = parse_json_document(&payload).map_err(|error| error.to_string())?;
        if text(&doc, "engine")? != ENGINE {
            return Err("checkpoint runner version is incompatible".into());
        }
        let function = text(&doc, "function")?;
        let mut given = BTreeMap::new();
        let JsonValue::Obj(entries) = field(&doc, "given")? else {
            return Err("invalid saved inputs".into());
        };
        for (name, value) in entries {
            let JsonValue::Str(value) = value else {
                return Err("invalid saved input value".into());
            };
            if given.insert(name.clone(), value.clone()).is_some() {
                return Err("duplicate saved input".into());
            }
        }
        let JsonValue::Arr(entries) = field(&doc, "completed")? else {
            return Err("invalid completed cases".into());
        };
        let mut completed = Vec::with_capacity(entries.len());
        for value in entries {
            let JsonValue::Str(value) = value else {
                return Err("invalid saved result document".into());
            };
            let result = parse_json_document(value).map_err(|error| error.to_string())?;
            if !matches!(field(&result, "goal_met")?, JsonValue::Bool(_)) {
                return Err("invalid saved goal status".into());
            }
            text(&result, "status")?;
            completed.push(value.clone());
        }
        let total = usize::try_from(doc.int_field("total").map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        if completed.len() > total {
            return Err("checkpoint completed-case count exceeds its target".into());
        }
        let measure = usize::try_from(
            doc.int_field("measure")
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let JsonValue::Arr(entries) = field(&doc, "measurements")? else {
            return Err("invalid measurements".into());
        };
        let mut measurements = Vec::with_capacity(entries.len());
        for entry in entries {
            let JsonValue::Str(entry) = entry else {
                return Err("invalid measurement document".into());
            };
            parse_json_document(entry).map_err(|error| error.to_string())?;
            measurements.push(entry.clone());
        }
        let revision = unsigned(&doc, "revision")?;
        if revision < completed.len() { return Err("revision precedes completed cases".into()); }
        let active_result = text(&doc, "active_result")?;
        let active_result = if active_result.is_empty() { None } else {
            let result = parse_json_document(&active_result).map_err(|error| error.to_string())?;
            if field(&result, "goal_met")? != &JsonValue::Bool(false) { return Err("unfinished method claims its goal is met".into()); }
            Some(active_result)
        };
        let JsonValue::Obj(entries) = field(&doc, "methods")? else { return Err("invalid saved methods".into()); };
        let mut methods = BTreeMap::new();
        for (case, frames) in entries {
            let case = case.parse::<usize>().map_err(|error| error.to_string())?;
            let frames = saved_frames(frames)?;
            if case > completed.len() || case >= total || frames.is_empty() || methods.insert(case, frames).is_some() { return Err("invalid method case index".into()); }
        }
        if active_result.is_some() != methods.contains_key(&completed.len()) { return Err("active result and method state disagree".into()); }
        if measure > 32
            || (measure == 0 && !measurements.is_empty())
            || (measure > 0 && measurements.len() != revision)
        {
            return Err("measurement policy or completed sample count differs".into());
        }
        let mut state = Self {
            source_path: PathBuf::from(text(&doc, "source_path")?),
            source: text(&doc, "source")?,
            meaning_id: text(&doc, "meaning_id")?,
            language_id: text(&doc, "language_id")?,
            function: (!function.is_empty()).then_some(function),
            given,
            completed,
            active_result,
            methods,
            revision,
            total,
            measure,
            measurements,
            branch: None,
        };
        let branch = field(&doc, "branch")?;
        if let Ok(parent) = branch.string_field("parent") {
            let relation = text(branch, "relation")?;
            let parent_path = PathBuf::from(parent);
            let parent = Self::load_at(&parent_path, depth + 1)?;
            state.attach_branch(parent_path, &parent, &relation)?;
        } else if !matches!(branch, JsonValue::Obj(entries) if entries.is_empty()) {
            return Err("invalid branch ancestry".into());
        }
        if state.payload() != payload { return Err("checkpoint payload is not canonical".into()); }
        Ok(state)
    }

    fn attach_branch(
        &mut self,
        path: PathBuf,
        parent: &Self,
        relation: &str,
    ) -> Result<(), String> {
        if !valid_relation(relation) {
            return Err("unknown target relation".into());
        }
        let same = self.target_id() == parent.target_id();
        if relation == "same-problem" && !same {
            return Err("same-problem cannot change source, inputs, domain, constraints, or tolerance; choose an explicit changed-problem relation".into());
        }
        self.branch = Some(Branch {
            parent: path,
            relation: relation.into(),
            original_target: parent.original_target(),
            preserves_original: relation == "same-problem" && parent.preserves_original(),
        });
        Ok(())
    }
}

fn field<'a>(doc: &'a JsonValue, name: &str) -> Result<&'a JsonValue, String> {
    doc.field(name).map_err(|error| error.to_string())
}
fn text(doc: &JsonValue, name: &str) -> Result<String, String> {
    doc.string_field(name).map_err(|error| error.to_string())
}

pub(crate) fn diagnostic(json: bool, exit: CliExit, code: &str, message: &str) -> CliExit {
    eprintln!("error: {code}: {message}");
    if json {
        let mut out = JsonWriter::object();
        out.string("schema", SCHEMA);
        out.string("operation_status", "failed");
        out.bool("goal_met", false);
        out.objects("results", &[]);
        out.objects(
            "diagnostics",
            &[crate::json_diagnostic_entry(code, "error", message)],
        );
        println!("{}", out.finish());
    }
    exit
}

fn checked_package(path: &Path, json: bool) -> Result<SemanticPackage, CliExit> {
    let mut session = CompilerSession::new(Limits::default());
    let loaded = session
        .load_package(path)
        .map_err(|error| diagnostic(json, EXIT_USAGE, "E-PKG-080", &format!("{error:?}")))?;
    let result = session.check(loaded.file);
    if result.diagnostics.has_errors() {
        crate::print_diagnostics(&result.diagnostics);
        if json {
            let mut out = JsonWriter::object();
            out.string("schema", SCHEMA);
            out.string("operation_status", "failed");
            out.bool("goal_met", false);
            out.objects("results", &[]);
            out.objects(
                "diagnostics",
                &crate::json_diagnostics_entries(&result.diagnostics),
            );
            println!("{}", out.finish());
        }
        Err(EXIT_REFUSED)
    } else {
        Ok(result.package)
    }
}

fn installed_language(path: &Path, json: bool) -> Result<String, CliExit> {
    let distribution = crate::cli_dispatch::load_verified_language(Some(path))
        .map_err(|detail| diagnostic(json, EXIT_REFUSED, "E-LANG-IMAGE", &detail))?;
    Ok(distribution.image.distribution_hash.to_string())
}

pub(crate) fn run(request: RunRequest) -> CliExit {
    if let Some(exit) = crate::refuse_malformed_project_lock(&request.path) {
        return diagnostic(
            request.json,
            exit,
            "E-RUN-LOCK",
            "the project meaning lock refused this source; inspect emath-lab meaning explain",
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
    let package = match checked_package(&path, request.json) {
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

fn resume_package(state: &SavedRun, json: bool) -> Result<SemanticPackage, CliExit> {
    if let Some(exit) = crate::refuse_malformed_project_lock(&state.source_path) {
        return Err(diagnostic(
            json,
            exit,
            "E-RUN-LOCK",
            "the project meaning lock refused this source; inspect emath-lab meaning explain",
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
fn jobs(
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

fn advance(
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

fn checkpoint_path(state: &SavedRun, directory: &Path) -> PathBuf {
    directory.join(format!(
        "{}.json",
        content_id_of_str(&state.payload()).0.replace(':', "-")
    ))
}

fn successor(state: &SavedRun, path: &Path) -> Result<Option<(SavedRun, PathBuf)>, String> {
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

fn execute_measured_case(
    package: &SemanticPackage,
    declaration: &emath_ir::Declaration,
    test: Option<usize>,
    raw: &BTreeMap<String, String>,
    repetitions: usize,
    previous: &[MethodFrame],
) -> Result<(String, Option<String>, Vec<MethodFrame>), String> {
    if repetitions == 0 {
        return execute_case(package, declaration, test, raw, previous, true).map(|(result, frames)| (result, None, frames));
    }
    let mut samples = Vec::with_capacity(repetitions);
    let mut answer = None;
    for _ in 0..repetitions {
        let start = Instant::now();
        let result = execute_case(package, declaration, test, raw, previous, true)?;
        let elapsed = u64::try_from(start.elapsed().as_nanos())
            .map_err(|_| "execution duration exceeds u64 nanoseconds")?;
        if answer.as_ref().is_some_and(|prior| prior != &result) {
            return Err("E-MEASURE-RESULT: repeated execution changed the result; no latency comparison is admitted".into());
        }
        samples.push(elapsed);
        if answer.is_none() {
            answer = Some(result);
        }
    }
    let measurement = emath_lab_core::measure::Measurement {
        metric_id: "reference-case".into(),
        kind: emath_lab_core::measure::MeasurementKind::LatencyNs,
        unit: "ns".into(),
        samples,
    };
    let summary = measurement.summarize().map_err(|error| error.to_string())?;
    let mut out = JsonWriter::object();
    out.string("kind", measurement.kind.as_str());
    out.string("unit", &measurement.unit);
    out.string("engine", ENGINE);
    out.string("os", std::env::consts::OS);
    out.string("arch", std::env::consts::ARCH);
    out.string("scope", "reference case binding, lowering, execution and value rendering; excludes admission and storage; not generated-code performance");
    out.string(
        "evidence",
        "observed wall time; not a certified bound or speedup claim",
    );
    out.strings(
        "samples_ns",
        &measurement
            .samples
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>(),
    );
    out.string("median_ns", &summary.median.to_string());
    out.bool("quarantined", summary.quarantined());
    let (answer, frames) = answer.ok_or("measurement needs at least one execution")?;
    Ok((answer, Some(out.finish()), frames))
}

fn execute_case(
    package: &SemanticPackage,
    declaration: &emath_ir::Declaration,
    test: Option<usize>,
    raw: &BTreeMap<String, String>,
    previous: &[MethodFrame],
    advance: bool,
) -> Result<(String, Vec<MethodFrame>), String> {
    let (result, frames) = progress::with_frames(previous, advance, || {
    let result = if let Some(index) = test {
        let test = package
            .tests
            .get(index)
            .ok_or("saved example index is invalid")?;
        run_test(package, declaration, test)
    } else {
        let mut given = BTreeMap::new();
        for (name, value) in raw {
            let field = declaration
                .inputs
                .iter()
                .chain(&declaration.state)
                .chain(
                    declaration
                        .constructors
                        .iter()
                        .flat_map(|constructor| &constructor.parameters),
                )
                .find(|field| field.name == *name)
                .ok_or_else(|| {
                    format!("'{name}' is not a declared input, state, or constructor parameter")
                })?;
            let ty = package
                .ty(field.ty)
                .ok_or("declared input type is absent")?;
            let value = parse_set_value_for(Some(ty), value).ok_or_else(|| format!("cannot bind '{name}' to {}; check the value and vector length; other carriers need a source example", ty.display_name()))?;
            given.insert(name.clone(), value);
        }
        run_direct(package, declaration, &given)
    };
    Ok::<_, String>(result)
    });
    let result = result?;
    Ok((result_json(package, declaration, &result, &frames), frames))
}

fn result_json(
    package: &SemanticPackage,
    declaration: &emath_ir::Declaration,
    run: &TestRun,
    frames: &[MethodFrame],
) -> String {
    let mut remaining = Vec::new();
    for id in &declaration.goals {
        if let Some(goal) = package.goal(*id) {
            if goal.kind != GoalKind::Evaluate {
                remaining.push(format!(
                    "{} {}: this case computed definitions, not this separate goal",
                    goal.kind.as_str(),
                    goal.target
                ));
            }
        }
    }
    for output in &declaration.outputs {
        if !run.outputs.contains_key(&output.name)
            && !run.definitions.contains_key(&output.name)
            && !run.state.contains_key(&output.name)
        {
            remaining.push(format!("output '{}' has no computed value", output.name));
        }
    }
    for frame in frames {
        if progress::complete(&frame.state) != Some(true) {
            remaining.push(format!("{}: the saved method has not met its mathematical goal", frame.capability));
        }
        if let Some(fault) = &frame.fault { remaining.push(fault.clone()); }
    }
    let success = matches!(run.verdict, TestVerdict::Passed | TestVerdict::Computed);
    let has_result = !run.definitions.is_empty()
        || !run.outputs.is_empty()
        || !run.state.is_empty()
        || run.verdict.expect_passed();
    let mut out = JsonWriter::object();
    out.string("declaration", declaration.name.leaf());
    out.string("example", &run.name);
    out.string("relation", "same-problem");
    out.string(
        "status",
        if run.verdict.is_symbolic() {
            "unresolved"
        } else if success && has_result {
            "computed"
        } else {
            "failed"
        },
    );
    out.bool("goal_met", success && has_result && remaining.is_empty());
    out.string(
        "evidence",
        if run.verdict.expect_passed() {
            "source-example-checked"
        } else {
            "reference-execution;not-a-proof"
        },
    );
    out.object_field("inputs", &values_json(&run.given));
    if !run.state.is_empty() {
        out.object_field("state", &values_json(&run.state));
    }
    out.object_field(
        "outputs",
        &values_json(if run.outputs.is_empty() {
            &run.definitions
        } else {
            &run.outputs
        }),
    );
    if let TestVerdict::Symbolic { forms, holes, .. } = &run.verdict {
        let mut symbolic = JsonWriter::object();
        for (name, form) in forms {
            symbolic.string(name, form);
        }
        out.object_field("symbolic", &symbolic.finish());
        let mut required = JsonWriter::object();
        for (name, ty) in holes {
            required.string(name, ty);
        }
        out.object_field("required_inputs", &required.finish());
    }
    if let Some(reason) = run.verdict.reason_text() {
        remaining.push(reason);
    }
    if matches!(run.verdict, TestVerdict::Failed) {
        remaining.push("the source expectation evaluated to false".into());
    }
    if !has_result {
        remaining.push("no mathematical value was produced by this case".into());
    }
    out.strings("remaining", &remaining);
    out.finish()
}

fn values_json(values: &BTreeMap<String, Value>) -> String {
    let mut out = JsonWriter::object();
    for (name, value) in values { out.object_field(name, &value.json()); }
    out.finish()
}

fn unsigned(doc: &JsonValue, name: &str) -> Result<usize, String> {
    usize::try_from(doc.int_field(name).map_err(|error| error.to_string())?).map_err(|error| error.to_string())
}

fn saved_value(doc: &JsonValue) -> Result<Value, String> {
    let invalid = || "E-RUN-VALUE: invalid exact checkpoint value".to_string();
    let boolean = |name| match field(doc, name)? { JsonValue::Bool(value) => Ok(*value), _ => Err(invalid()) };
    let float = |bits: &str| {
        if bits.len() != 16 { return Err(invalid()); }
        u64::from_str_radix(bits, 16).map(f64::from_bits).map_err(|_| invalid())
    };
    let floats = |name| doc.strings_field(name).map_err(|error| error.to_string())?.iter().map(|bits| float(bits)).collect::<Result<Vec<_>, _>>();
    let elements = || match field(doc, "elements")? {
        JsonValue::Arr(values) => values.iter().map(saved_value).collect::<Result<Vec<_>, _>>(), _ => Err(invalid()),
    };
    let value = match text(doc, "type")?.as_str() {
        "Int" => Value::I64(text(doc, "value")?.parse().map_err(|_| invalid())?),
        "Float64" => Value::F64(float(&text(doc, "bits")?)?),
        "Bool" => Value::Bool(text(doc, "value")?.parse().map_err(|_| invalid())?),
        "Text" => Value::Text(text(doc, "value")?),
        "BigInt" => Value::parse_bigint(&text(doc, "value")?).ok_or_else(invalid)?,
        "Rat" => {
            let num = text(doc, "numerator")?.parse::<i128>().map_err(|_| invalid())?;
            let den = text(doc, "denominator")?.parse::<i128>().map_err(|_| invalid())?;
            let ratio = emath_rt::ratio_norm((num, den))?;
            if ratio != (num, den) { return Err(invalid()); }
            Value::Rat { num, den }
        }
        "Record" => {
            let JsonValue::Obj(entries) = field(doc, "fields")? else { return Err(invalid()); };
            let mut fields = BTreeMap::new();
            for (name, value) in entries {
                if fields.insert(name.clone(), saved_value(value)?).is_some() { return Err(invalid()); }
            }
            Value::Record { type_name: text(doc, "name")?, fields }
        }
        "List" => Value::List(elements()?),
        "Set" => Value::Set(elements()?),
        "Vector<Float64>" => Value::Vector(floats("bits")?),
        "Vector<BigInt>" => Value::BigVector(doc.strings_field("elements").map_err(|error| error.to_string())?.iter().map(|value| {
            match Value::parse_bigint(value) { Some(Value::BigInt(value)) => Ok(value), _ => Err(invalid()) }
        }).collect::<Result<Vec<_>, _>>()?),
        "Matrix<Float64>" => {
            let rows = unsigned(doc, "rows")?;
            let cols = unsigned(doc, "columns")?;
            let data = floats("bits")?;
            if rows.checked_mul(cols) != Some(data.len()) { return Err(invalid()); }
            Value::Matrix { rows, cols, data }
        }
        "Tensor<Float64>" => {
            let shape = doc.strings_field("shape").map_err(|error| error.to_string())?.iter().map(|size| size.parse::<usize>().map_err(|_| invalid())).collect::<Result<Vec<_>, _>>()?;
            let data = floats("bits")?;
            if shape.iter().try_fold(1_usize, |size, dimension| size.checked_mul(*dimension)) != Some(data.len()) { return Err(invalid()); }
            Value::Tensor { shape, data }
        }
        "Complex" | "Interval<Float64>" => {
            let data = floats("bits")?;
            let [left, right] = data.as_slice() else { return Err(invalid()); };
            if text(doc, "type")? == "Complex" { Value::Complex { re: *left, im: *right } }
            else { Value::Interval { lo: *left, hi: *right } }
        }
        "Option" => Value::Option(if boolean("some")? { Some(Box::new(saved_value(field(doc, "payload")?)?)) } else { None }),
        "Result" => Value::Result { ok: boolean("ok")?, payload: Box::new(saved_value(field(doc, "payload")?)?) },
        "Series" => {
            let times = floats("time_bits")?;
            let values = floats("value_bits")?;
            if times.len() != values.len() { return Err(invalid()); }
            Value::Series { points: times.into_iter().zip(values).collect(), interpolation: text(doc, "interpolation")?, extrapolation: text(doc, "extrapolation")? }
        }
        _ => return Err("E-RUN-VALUE: this carrier cannot be saved as a method argument".into()),
    };
    if value.to_string() != text(doc, "value")? { return Err(invalid()); }
    Ok(value)
}

fn frames_json(frames: &[MethodFrame]) -> Vec<String> {
    frames.iter().map(|frame| {
        let mut out = JsonWriter::object();
        out.string("key", &frame.key);
        out.string("capability", &frame.capability);
        out.objects("arguments", &frame.arguments.iter().map(Value::json).collect::<Vec<_>>());
        out.object_field("state", &frame.state.json());
        if let Some(fault) = &frame.fault { out.string("fault", fault); } else { out.field("fault", "null"); }
        out.finish()
    }).collect()
}

fn saved_frames(doc: &JsonValue) -> Result<Vec<MethodFrame>, String> {
    let JsonValue::Arr(entries) = doc else { return Err("invalid method frames".into()); };
    let mut frames: Vec<MethodFrame> = Vec::new();
    for entry in entries {
        let key = text(entry, "key")?;
        if frames.iter().any(|frame| frame.key == key) { return Err("duplicate method frame".into()); }
        let JsonValue::Arr(arguments) = field(entry, "arguments")? else { return Err("invalid method arguments".into()); };
        let fault = match field(entry, "fault")? { JsonValue::Str(fault) => Some(fault.clone()), JsonValue::Null => None, _ => return Err("invalid method fault".into()) };
        frames.push(MethodFrame { key, capability: text(entry, "capability")?, arguments: arguments.iter().map(saved_value).collect::<Result<_, _>>()?, state: saved_value(field(entry, "state")?)?, fault });
    }
    Ok(frames)
}

fn save(state: &SavedRun, destination: &Path) -> Result<(), String> {
    let payload = state.payload();
    let mut envelope = JsonWriter::object();
    envelope.string("schema", CHECKPOINT_SCHEMA);
    envelope.string("content_id", &content_id_of_str(&payload).0);
    envelope.string("payload", &payload);
    save_document(&envelope.finish(), destination)
}

fn save_document(bytes: &str, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or("checkpoint has no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staging = parent.join(format!(
        ".run-{}-{}.tmp",
        std::process::id(),
        STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|error| error.to_string())?;
    let written = file
        .write_all(bytes.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| std::fs::hard_link(&staging, destination));
    // Only this invocation's staging file is removed. Published checkpoints
    // are immutable; concurrent requests cannot replace one another's data.
    let _ = std::fs::remove_file(&staging);
    match result {
        Ok(()) => std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read_to_string(destination).ok().as_deref() == Some(bytes) {
                Ok(())
            } else {
                Err("checkpoint publication conflicts with an existing result".into())
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

fn emit(
    state: &SavedRun,
    path: &Path,
    json: bool,
    issue: Option<(CliExit, &str, &str)>,
) -> CliExit {
    if let Some((_, code, message)) = issue {
        eprintln!("error: {code}: {message}");
    }
    let parsed: Result<Vec<_>, _> = state
        .completed
        .iter()
        .chain(state.active_result.iter())
        .map(|value| parse_json_document(value))
        .collect();
    let results = match parsed {
        Ok(results) => results,
        Err(error) => return diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error.to_string()),
    };
    let current_target_met = state.completed.len() == state.total
        && state.total > 0
        && results
            .iter()
            .all(|result| matches!(result.field("goal_met"), Ok(JsonValue::Bool(true))));
    let goal_met = current_target_met && state.preserves_original();
    let failed = results
        .iter()
        .any(|result| result.string_field("status").ok().as_deref() == Some("failed"));
    let mut out = JsonWriter::object();
    out.string("schema", SCHEMA);
    out.string(
        "operation_status",
        if issue.is_some_and(|(_, code, _)| code == "E-RUN-CANCELLED") {
            "cancelled"
        } else if failed || issue.is_some() {
            "partial-failed"
        } else {
            "completed"
        },
    );
    out.bool("goal_met", goal_met);
    out.bool("current_target_met", current_target_met);
    out.string("target_id", &state.target_id());
    out.string("original_target_id", &state.original_target());
    out.string(
        "relation",
        state
            .branch
            .as_ref()
            .map_or("same-problem", |branch| branch.relation.as_str()),
    );
    out.string("result_relation_scope", "current-target");
    out.object_field("branch", &state.branch_json());
    out.objects("measurements", &state.measurements);
    let diagnostics =
        issue.map(|(_, code, message)| crate::json_diagnostic_entry(code, "error", message));
    out.objects("diagnostics", &diagnostics.into_iter().collect::<Vec<_>>());
    out.string("meaning_id", &state.meaning_id);
    out.string("language_id", &state.language_id);
    out.string("engine", ENGINE);
    out.string("checkpoint", &path.to_string_lossy());
    out.int("revision", state.revision as u64);
    out.string("work_unit", "case-initialization-or-authored-method-step");
    out.int("completed_cases", state.completed.len() as u64);
    out.int(
        "remaining_cases",
        state.total.saturating_sub(state.completed.len()) as u64,
    );
    let visible_results: Vec<_> = state.completed.iter().chain(state.active_result.iter()).cloned().collect();
    out.objects("results", &visible_results);
    let mut next = Vec::new();
    if state.completed.len() < state.total {
        let claimed = std::fs::read_to_string(path.with_extension("request.json"))
            .ok()
            .and_then(|bytes| parse_json_document(&bytes).ok());
        let origin = claimed
            .as_ref()
            .and_then(|claim| claim.string_field("origin").ok())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let revision = claimed
            .as_ref()
            .and_then(|claim| claim.int_field("revision").ok())
            .unwrap_or(state.revision as u64);
        let work = claimed
            .as_ref()
            .and_then(|claim| claim.int_field("work").ok())
            .unwrap_or(
                state
                    .total
                    .saturating_sub(state.completed.len())
                    .min(DEFAULT_WORK) as u64,
            );
        let mut action = JsonWriter::object();
        action.string("operation", "step");
        action.string("target_relation", "same-target");
        action.strings(
            "argv",
            &[
                "emath".into(),
                "step".into(),
                origin,
                "--expect-revision".into(),
                revision.to_string(),
                "--work".into(),
                work.to_string(),
                "--json".into(),
            ],
        );
        next.push(action.finish());
    }
    out.objects("next", &next);
    if json {
        println!("{}", out.finish());
    } else {
        for result in &results {
            println!(
                "{} / {}: {}",
                text(result, "declaration").unwrap_or_default(),
                text(result, "example").unwrap_or_default(),
                text(result, "status").unwrap_or_default()
            );
            if let Ok(JsonValue::Obj(outputs)) = result.field("outputs") {
                for (name, value) in outputs {
                    println!(
                        "  {name} = {} [{}]",
                        text(value, "value").unwrap_or_default(),
                        text(value, "type").unwrap_or_default()
                    );
                }
            }
            if let Ok(JsonValue::Obj(state)) = result.field("state") {
                for (name, value) in state {
                    println!(
                        "  state.{name} = {} [{}]",
                        text(value, "value").unwrap_or_default(),
                        text(value, "type").unwrap_or_default()
                    );
                }
            }
            for label in ["symbolic", "required_inputs"] {
                if let Ok(JsonValue::Obj(entries)) = result.field(label) {
                    for (name, value) in entries {
                        if let JsonValue::Str(value) = value {
                            println!("  {label}: {name} = {value}");
                        }
                    }
                }
            }
            if let Ok(JsonValue::Arr(remaining)) = result.field("remaining") {
                for value in remaining {
                    if let JsonValue::Str(value) = value {
                        println!("  remaining: {value}");
                    }
                }
            }
        }
        println!(
            "goal_met={goal_met}; current_target_met={current_target_met}; completed={}/{}; checkpoint={}",
            state.completed.len(),
            state.total,
            path.display()
        );
        if let Some(branch) = &state.branch {
            println!(
                "relation={}; original_target={}; no results merge into the parent",
                branch.relation, branch.original_target
            );
        }
        for measurement in &state.measurements {
            println!("measurement: {measurement}");
        }
        if state.completed.len() < state.total {
            println!("next: emath inspect {:?} --json", path);
        }
    }
    if let Some((exit, _, _)) = issue {
        exit
    } else if goal_met {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

pub(crate) fn inspect(path: &Path, json: bool) -> CliExit {
    match SavedRun::load(path) {
        Ok(mut state) => {
            let mut path = path.to_path_buf();
            loop {
                match successor(&state, &path) {
                    Ok(Some((saved, destination))) => {
                        state = saved;
                        path = destination;
                    }
                    Ok(None) => break,
                    Err(error) => return diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error),
                }
            }
            emit(&state, &path, json, None);
            EXIT_OK
        }
        Err(error) => diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error),
    }
}

pub(crate) fn verify(path: &Path, json: bool) -> CliExit {
    let state = match SavedRun::load(path) {
        Ok(state) => state,
        Err(error) => return diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error),
    };
    let package = match resume_package(&state, json) {
        Ok(package) => package,
        Err(exit) => return exit,
    };
    let jobs = match jobs(&package, &state) {
        Ok(jobs) if jobs.len() == state.total => jobs,
        _ => {
            return diagnostic(
                json,
                EXIT_REFUSED,
                "E-RUN-STATE",
                "saved case plan differs from the admitted source",
            );
        }
    };
    let checked_cases = state.completed.len() + usize::from(state.active_result.is_some());
    for (index, &(declaration, test)) in jobs.iter().take(checked_cases).enumerate() {
        let frames = state.methods.get(&index).map(Vec::as_slice).unwrap_or(&[]);
        let result = execute_case(
            &package,
            &package.declarations[declaration],
            test,
            &state.given,
            frames,
            false,
        );
        let expected = state.completed.get(index).or_else(|| state.active_result.as_ref());
        if !result.as_ref().is_ok_and(|(result, observed)| Some(result) == expected && observed == frames) {
            return diagnostic(
                json,
                EXIT_REFUSED,
                "E-RUN-VERIFY",
                &format!("certificate checking or source reconstruction disagrees with saved case {index}"),
            );
        }
    }
    if json {
        let mut out = JsonWriter::object();
        out.string("schema", "emath.run-verification.v1");
        out.bool("verified", true);
        out.string("target_id", &state.target_id());
        out.bool("original_target_preserved", state.preserves_original());
        if state.measure > 0 {
            out.bool("measurements_verified", false);
        }
        out.string(
            "scope",
            "authored certificate checks and source result reconstruction; no refinement replay, measurement proof, or execution-history claim",
        );
        out.int("checked_cases", checked_cases as u64);
        out.int("checked_methods", state.methods.values().map(Vec::len).sum::<usize>() as u64);
        println!("{}", out.finish());
    } else {
        println!(
            "verified {} cases by certificate checks and source reconstruction; not a formal proof",
            checked_cases
        );
    }
    EXIT_OK
}

/// Shared CLI literal parser. Exact integer inputs never pass through f64.
pub fn parse_set_value_for(declared: Option<&TypeNode>, raw: &str) -> Option<Value> {
    let value = match declared {
        Some(TypeNode::BigInt) => Value::parse_bigint(raw),
        Some(TypeNode::Int) => raw.trim().parse::<i64>().ok().map(Value::I64),
        Some(TypeNode::Nat) => raw
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|value| *value >= 0)
            .map(Value::I64),
        Some(TypeNode::Bool) => raw.trim().parse::<bool>().ok().map(Value::Bool),
        Some(TypeNode::Vector { element, .. })
            if matches!(&**element, TypeNode::Int | TypeNode::Nat) =>
        {
            let inner = raw.trim().strip_prefix('[')?.strip_suffix(']')?;
            let values: Option<Vec<f64>> = inner
                .split(',')
                .map(|part| {
                    let value = part.trim().parse::<i64>().ok()?;
                    if !(-9_007_199_254_740_992..=9_007_199_254_740_992).contains(&value)
                        || (matches!(&**element, TypeNode::Nat) && value < 0)
                    {
                        return None;
                    }
                    Some(value as f64)
                })
                .collect();
            values
                .filter(|values| !values.is_empty())
                .map(Value::Vector)
        }
        _ => parse_set_value(raw),
    }?;
    declared
        .is_none_or(|ty| binding_matches(ty, &value))
        .then_some(value)
}

/// Existing finite scalar/vector CLI vocabulary; source examples retain
/// the language's complete literal and expression vocabulary.
pub fn parse_set_value(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if let Some(inner) = raw
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let values: Option<Vec<f64>> = inner
            .split(',')
            .map(|part| {
                part.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
            })
            .collect();
        return values
            .filter(|values| !values.is_empty())
            .map(Value::Vector);
    }
    raw.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(Value::F64)
}

fn binding_matches(ty: &TypeNode, value: &Value) -> bool {
    match (ty, value) {
        (TypeNode::Float64, Value::F64(_))
        | (TypeNode::Int, Value::I64(_))
        | (TypeNode::BigInt, Value::BigInt(_))
        | (TypeNode::Bool, Value::Bool(_)) => true,
        (TypeNode::Nat, Value::I64(value)) => *value >= 0,
        (TypeNode::Vector { element, extent }, Value::Vector(values)) => {
            matches!(
                &**element,
                TypeNode::Float64 | TypeNode::Int | TypeNode::Nat
            ) && match extent {
                Some(emath_ir::Extent::Fixed(size)) => values.len() == *size,
                None => true,
                Some(emath_ir::Extent::Symbolic(_)) => false,
            }
        }
        _ => false,
    }
}

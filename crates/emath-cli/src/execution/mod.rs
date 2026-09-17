//! Answer-first execution over the existing declaration runner.
//!
//! A work unit initializes one case or advances one authored method.
//! Each unit commits before the next starts. OS locks serialize revisions.
//! Checkpoints retain exact values and capsule-owned continuation contracts.
//! Source, Language Image, bindings and runner version stay fixed on resume.

use crate::{
    CliExit, EXIT_ADMISSION, EXIT_FAULT, EXIT_OK, EXIT_PARTIAL, EXIT_REFUSED, EXIT_USAGE,
};
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
    pub set_file: Option<PathBuf>,
    pub work: usize,
    pub work_set: bool,
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
        let mut set_file = None;
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
                "--set-file" if !continuation => {
                    crate::assign_once(
                        &mut set_file,
                        PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                    )?;
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
            set_file,
            work: work.unwrap_or(DEFAULT_WORK),
            work_set: work.is_some(),
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
            if matches!(result.field("goal_met"), Ok(JsonValue::Bool(true))) {
                return Err("unfinished method claims a completed constructor answer".into());
            }
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

fn case_computed(result: &JsonValue) -> bool {
    result.string_field("status").ok().as_deref() == Some("computed")
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
        out.string("schema_version", "emath.constructor.v1");
        out.string("command", "run");
        out.string("admission", "refused");
        out.string("execution", "absent");
        out.string("fulfillment", "unmet");
        out.objects("evidence", &[]);
        out.strings("remaining", &[message.to_string()]);
        out.objects(
            "diagnostics",
            &[crate::json_diagnostic_entry(code, "error", message)],
        );
        println!("{}", out.finish());
    }
    exit
}


// Split out of the original single file. `prelude` reexports the
// children at module width so they can share helpers; the `pub use`
// lines preserve the original crate-visible surface.
use prelude::*;
mod prelude;
mod advance;
mod case;
mod cmd;
mod constructor;
mod emit;
mod package;
mod run;
mod save;

pub use cmd::*;
pub(crate) use run::*;


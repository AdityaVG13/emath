//! The level chain: a closed level writes the next level's source
//! inside the run directory, verifies it, and the run continues on it.
//!
//! The seam is an authored pair:
//! - `emath function dream_carry` (one `Int` input, one output of any
//!   literal-renderable type C: Int, Rat, Bool, sequences, records, and
//!   quoted `Code`, written back as `quote(<source>)`): what a level
//!   starts from;
//! - `emath function dream_next` (one input of the state type, one
//!   output of type C): what the closing state says the next level
//!   should start from.
//!
//! At a close, `dream_next(final state)` is compared with
//! `dream_carry(0)`. Equal values are a fixed point and the run closes
//! as a single level would. Otherwise the current level's source is
//! copied with `dream_carry`'s definitions replaced by one binding of
//! the new value, written to `world/level_NNN.emath`, and verified: it
//! must parse, admit with the same Step surface, and pass every
//! authored test. Only a verified file becomes the next level; a
//! rejected file stays on disk for inspection and the run closes.
//!
//! Write scope: the driver writes only under the run directory. The
//! launch module and its local imports are copied into `world/` when
//! the run is created and never read again; `language/` is imported,
//! never written.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use emath_core::tree::{Item, SyntaxTree};
use emath_exec_ir::constructor_layer::{evaluate_tree_budgeted_at, named_programs, needed_uses, place_definition, prepare_module, CValue, Placed,
};
use emath_exec_ir::exact_int::ExactInt;
use emath_syntax::parse_str;

use super::journal::io_fault;
use super::rundir::write_atomic;
use crate::host::{section_fields, HostFault, LoopHost};

pub const CARRY: &str = "dream_carry";
pub const NEXT: &str = "dream_next";

/// The resolved seam: the author-chosen input and output names.
pub struct ChainSeam {
    carry_input: String,
    carry_output: String,
    next_input: String,
}

fn function<'a>(tree: &'a SyntaxTree, name: &str) -> Option<&'a emath_core::tree::Declaration> {
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(decl) if decl.as_kind == "function" && decl.name == name => Some(decl),
        _ => None,
    })
}

/// The seam, `None` when the module declares neither function, or a
/// named refusal when it is half-authored or mis-shaped.
pub fn resolve(host: &LoopHost) -> Result<Option<ChainSeam>, HostFault> {
    let tree = host.tree();
    let (carry, next) = match (function(tree, CARRY), function(tree, NEXT)) {
        (None, None) => return Ok(None),
        (Some(carry), Some(next)) => (carry, next),
        _ => {
            return Err(HostFault::fault(
                "dream_chain_surface",
                "the level chain needs both `emath function dream_carry` (one Int input, one \
                 output) and `emath function dream_next` (one input of the state type, the same \
                 output type)",
            ));
        }
    };
    let carry_in = section_fields(carry, "inputs");
    let carry_out = section_fields(carry, "outputs");
    let next_in = section_fields(next, "inputs");
    let next_out = section_fields(next, "outputs");
    let state = &host.surface().state_type;
    let shaped = carry_in.len() == 1
        && carry_in[0].1.as_deref() == Some("Int")
        && carry_out.len() == 1
        && next_in.len() == 1
        && next_in[0].1.as_deref() == Some(state.as_str())
        && next_out.len() == 1
        && next_out[0].1 == carry_out[0].1;
    if !shaped {
        return Err(HostFault::fault(
            "dream_chain_surface",
            format!(
                "dream_carry must be (Int) -> C and dream_next ({state}) -> C for one type C"
            ),
        ));
    }
    Ok(Some(ChainSeam {
        carry_input: carry_in[0].0.clone(),
        carry_output: carry_out[0].0.clone(),
        next_input: next_in[0].0.clone(),
    }))
}

impl ChainSeam {
    /// `dream_carry(0)` and `dream_next(state)` on this level, under the
    /// run's per-evaluation work allowance.
    pub fn values(&self, host: &LoopHost, state: &CValue) -> Result<(CValue, CValue), HostFault> {
        let zero = BTreeMap::from([(self.carry_input.clone(), CValue::Int(ExactInt::from(0)))]);
        let carry = host.evaluate(CARRY, &zero)?;
        let input = BTreeMap::from([(self.next_input.clone(), state.clone())]);
        let next = host.evaluate(NEXT, &input)?;
        Ok((carry, next))
    }

    /// The next level's source: `source` with `dream_carry`'s
    /// definitions replaced by one binding of `literal`.
    pub fn rewrite(&self, source: &str, literal: &str) -> Result<String, String> {
        let lines: Vec<&str> = source.lines().collect();
        let header = format!("emath function {CARRY}:");
        let start = lines
            .iter()
            .position(|line| line.trim_end() == header)
            .ok_or_else(|| format!("`{header}` is not a line of the level source"))?;
        let indent = |line: &str| line.len() - line.trim_start().len();
        let mut defs = None;
        for (index, line) in lines.iter().enumerate().skip(start + 1) {
            if !line.trim().is_empty() && indent(line) == 0 {
                break;
            }
            if line.trim() == "definitions:" {
                defs = Some(index);
                break;
            }
        }
        let defs = defs.ok_or_else(|| format!("`{CARRY}` has no definitions section"))?;
        let body_indent = indent(lines[defs]) + 4;
        let mut end = defs + 1;
        while end < lines.len() && (lines[end].trim().is_empty() || indent(lines[end]) >= body_indent) {
            end += 1;
        }
        while end > defs + 1 && lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        let mut out: Vec<String> = lines[..=defs].iter().map(|line| (*line).to_string()).collect();
        out.push(format!("{}{} = {literal}", " ".repeat(body_indent), self.carry_output));
        out.extend(lines[end..].iter().map(|line| (*line).to_string()));
        let mut text = out.join("\n");
        text.push('\n');
        Ok(text)
    }
}

pub const KEEP: &str = "dream_keep";

/// The run's own kept library, relative to the run directory.
pub const KEPT_LIBRARY: &str = "world/lib/kept.emath";

/// The keep seam: `emath function dream_keep` (one input of the state
/// type, one output) returns a sequence of `{name, program}` records
/// (the shape `emath keep` takes without `--name`). After the seed and
/// after every committed batch the driver keeps each name it has not
/// kept yet into [`KEPT_LIBRARY`], replacing an earlier definition of
/// the name (a replayed batch after a resume rewrites the same text).
pub struct KeepSeam {
    input: String,
}

/// The keep seam, `None` when the module declares no `dream_keep`.
pub fn resolve_keep(host: &LoopHost) -> Result<Option<KeepSeam>, HostFault> {
    let Some(keep) = function(host.tree(), KEEP) else {
        return Ok(None);
    };
    let inputs = section_fields(keep, "inputs");
    let outputs = section_fields(keep, "outputs");
    let state = &host.surface().state_type;
    if inputs.len() != 1 || inputs[0].1.as_deref() != Some(state.as_str()) || outputs.len() != 1 {
        return Err(HostFault::fault(
            "dream_keep_surface",
            format!("dream_keep must take one `{state}` input and return a sequence of {{name, program}} records"),
        ));
    }
    Ok(Some(KeepSeam { input: inputs[0].0.clone() }))
}

impl KeepSeam {
    /// Keep every program `dream_keep(state)` names that `known` lacks
    /// into `library`; returns the new `(name, content id)` pairs in
    /// the order the loop reported them.
    pub fn keep(
        &self,
        host: &LoopHost,
        state: &CValue,
        library: &Path,
        known: &std::collections::BTreeSet<String>,
        origin: &str,
    ) -> Result<Vec<(String, String)>, HostFault> {
        let input = BTreeMap::from([(self.input.clone(), state.clone())]);
        let value = host.evaluate(KEEP, &input)?;
        let fresh: Vec<(String, CValue)> =
            named_programs(&value)?.into_iter().filter(|(name, _)| !known.contains(name)).collect();
        if fresh.is_empty() {
            return Ok(Vec::new());
        }
        let mut text = match fs::read_to_string(library) {
            Ok(text) => Some(text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(io_fault("dream_io", library, &err)),
        };
        let mut kept = Vec::with_capacity(fresh.len());
        let mut changed = false;
        for (name, program) in fresh {
            let uses = needed_uses(&program, host.path(), library)?;
            let (placed, content) = place_definition(text.as_deref(), &name, &program, origin, true, &uses)?;
            if let Placed::Kept(updated) = placed {
                text = Some(updated);
                changed = true;
            }
            kept.push((name, content));
        }
        if let (true, Some(text)) = (changed, &text) {
            emath_syntax::install_source_parser();
            prepare_module(&parse_str(text).0, Some(library))?;
            if let Some(parent) = library.parent() {
                fs::create_dir_all(parent).map_err(|err| io_fault("dream_io", parent, &err))?;
            }
            write_atomic(library, text)?;
        }
        Ok(kept)
    }
}

/// A value as an authored literal, or why it has none.
pub fn literal(value: &CValue) -> Result<String, String> {
    Ok(match value {
        CValue::Int(n) => n.to_string(),
        CValue::Rat { num, den } => format!("({num} / {den})"),
        CValue::Bool(b) => b.to_string(),
        CValue::Sequence(items) => {
            let parts = items.iter().map(literal).collect::<Result<Vec<_>, _>>()?;
            format!("[{}]", parts.join(", "))
        }
        CValue::Record { type_name, fields } => {
            let parts = fields
                .iter()
                .map(|(name, field)| literal(field).map(|text| format!("{name}: {text}")))
                .collect::<Result<Vec<_>, _>>()?;
            format!("{type_name}: {{{}}}", parts.join(", "))
        }
        CValue::Code(code) => {
            let mut text = String::new();
            emath_syntax::formatter::format_expr(&mut text, &code.expr, emath_syntax::formatter::Prec::Root);
            format!("quote({})", text.trim())
        }
        other => return Err(format!("dream_next returned {other:?}, which has no authored literal")),
    })
}

/// Verify a written level: it opens as the same Step surface (parse,
/// admission, surface) and every authored test passes, each under the
/// run's per-evaluation work allowance.
pub fn verify(path: &Path, target: &str, work: u64) -> Result<(), String> {
    LoopHost::open(path, Some(target)).map_err(|fault| format!("{}: {}", fault.code, fault.message))?;
    let source = fs::read_to_string(path).map_err(|err| err.to_string())?;
    emath_syntax::install_source_parser();
    let (tree, diagnostics) = parse_str(&source);
    if diagnostics.has_errors() {
        return Err(format!("does not parse: {diagnostics:?}"));
    }
    let report = evaluate_tree_budgeted_at(&tree, Some(path), work).map_err(|err| format!("does not evaluate: {err}"))?;
    let failed: Vec<&str> = report.tests.iter().filter(|t| !t.passed).map(|t| t.label.as_str()).collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("authored tests failed: {}", failed.join(", ")))
    }
}

/// Copy `module` and every local file it imports (transitively) into
/// `world`, keeping their relative layout. A `use a.b` is local when
/// `<module dir>/a/b.emath` exists; anything else is a language import.
/// Returns the world copy of the module.
pub fn copy_world(module: &Path, world: &Path) -> Result<PathBuf, HostFault> {
    let root = module.parent().unwrap_or(Path::new("."));
    let name = module.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("world.emath"));
    let mut pending = vec![name.clone()];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(rel) = pending.pop() {
        if !seen.insert(rel.clone()) {
            continue;
        }
        let from = root.join(&rel);
        let text = fs::read_to_string(&from).map_err(|err| io_fault("loop_read", &from, &err))?;
        let to = world.join(&rel);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|err| io_fault("dream_io", parent, &err))?;
        }
        fs::write(&to, &text).map_err(|err| io_fault("dream_io", &to, &err))?;
        for line in text.lines() {
            let Some(path) = line.strip_prefix("use ").map(str::trim) else {
                continue;
            };
            let local = PathBuf::from(format!("{}.emath", path.replace('.', "/")));
            if root.join(&local).is_file() {
                pending.push(local);
            }
        }
    }
    Ok(world.join(name))
}

/// Copy a run's `world/` into another run (fork).
pub fn copy_dir(from: &Path, to: &Path) -> Result<(), HostFault> {
    let Ok(entries) = fs::read_dir(from) else {
        return Ok(());
    };
    fs::create_dir_all(to).map_err(|err| io_fault("dream_io", to, &err))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|err| io_fault("dream_io", &path, &err))?;
        }
    }
    Ok(())
}

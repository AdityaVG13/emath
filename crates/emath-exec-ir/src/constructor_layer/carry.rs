//! Carried facts: completed calls that outlive one evaluation.
//!
//! The engine memoizes every completed pure call inside one evaluation
//! (`call:<name>:<args>` keys). A function whose name starts with
//! `fact_` is a *fact function*: its completed calls are handed back to
//! the caller so a host can carry them into later evaluations and
//! persist them (the dream driver's evaluation ledger). A carried fact
//! is looked up exactly like an in-evaluation memo entry, so a hit
//! returns the value the call would compute.
//!
//! Soundness rests on two things. Calls are pure (the engine has no
//! ambient effects; buffers, closures, code, and receipts never form a
//! memo key). And a carried fact is only valid under the declarations
//! that produced it: `fact_identity` hashes every fact function, every
//! function it reaches, their input names and types, every object
//! schema, and the engine ABI. A host keys its store by that identity,
//! so an edit to anything a fact depends on starts a fresh store, while
//! edits elsewhere (comments, unrelated functions) keep it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::identity::{canonical_expr_bytes, free_path_names};
use super::prelude::*;
use super::setup::engine_from_tree_at;
use super::{CValue, ConstructorError, Engine, Expr, ExprKind, SyntaxTree, CHECKPOINT_ABI};

/// Name prefix of fact functions.
pub const FACT_PREFIX: &str = "fact_";

/// One evaluation's result plus the fact-function calls it completed
/// that were not already carried, and how many carried facts it used.
/// A faulted evaluation still reports its completed facts: a memo entry
/// exists only for a call that returned, so each is as valid as it
/// would be in a successful evaluation.
pub struct CarriedEvaluation {
    pub value: Result<CValue, ConstructorError>,
    pub fresh: Vec<(String, CValue)>,
    pub hits: u64,
}

pub(super) fn is_fact_key(key: &str) -> bool {
    key.strip_prefix("call:")
        .is_some_and(|rest| rest.starts_with(FACT_PREFIX))
        && !key.contains(":body=")
}

/// Evaluate a named function with `carried` facts pre-loaded into the
/// call memo, under `work_limit` engine steps. The evaluation never
/// suspends, so only fact-function calls are memoized. The outer error is a
/// setup failure (no evaluation ran); an evaluation fault is
/// `CarriedEvaluation::value`. A carried hit skips the call's work, so
/// a warm memo can finish under a limit a cold one exceeds: the limit
/// is a resource bound, not part of the result.
pub fn evaluate_function_carrying(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    source: Option<&Path>,
    carried: &BTreeMap<String, CValue>,
    work_limit: u64,
) -> Result<CarriedEvaluation, ConstructorError> {
    let mut engine = engine_from_tree_at(tree, source)?;
    engine.work_limit = work_limit;
    engine.facts_only_memo = true;
    engine.memo = carried.clone();
    let value = engine.eval_entry(name, inputs);
    let hits = engine.fact_hits.get();
    let fresh = std::mem::take(&mut engine.memo)
        .into_iter()
        .filter(|(key, _)| is_fact_key(key) && !carried.contains_key(key))
        .collect();
    Ok(CarriedEvaluation { value, fresh, hits })
}

/// The identity of the module's fact functions and everything they
/// depend on, as `emath:facts:v1:<sha256 hex>`; `None` when the module
/// declares no fact function.
pub fn fact_identity(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<Option<String>, ConstructorError> {
    let engine = engine_from_tree_at(tree, source)?;
    let mut pending: Vec<String> = engine
        .functions
        .keys()
        .filter(|name| name.starts_with(FACT_PREFIX))
        .cloned()
        .collect();
    if pending.is_empty() {
        return Ok(None);
    }
    let mut reached = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let Some(decl) = engine.functions.get(&name) else {
            continue;
        };
        let mut free = BTreeSet::new();
        for (_, expr) in &decl.defs {
            free_path_names(expr, &mut Vec::new(), &mut free);
        }
        pending.extend(
            free.into_iter()
                .filter(|dep| engine.functions.contains_key(dep) && !reached.contains(dep)),
        );
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(CHECKPOINT_ABI.as_bytes());
    for name in &reached {
        let decl = &engine.functions[name];
        bytes.push(b'F');
        bytes.extend_from_slice(name.as_bytes());
        for (input, ty) in decl.inputs.iter().zip(&decl.input_types) {
            bytes.push(b'i');
            bytes.extend_from_slice(input.as_bytes());
            bytes.push(b':');
            bytes.extend_from_slice(spanless(&format!("{:?}", ty.kind)).as_bytes());
        }
        for (def, expr) in &decl.defs {
            bytes.push(b'd');
            bytes.extend_from_slice(def.as_bytes());
            canonical_expr_bytes(expr, &mut Vec::new(), &mut bytes);
        }
    }
    for (name, schema) in &engine.objects {
        bytes.push(b'O');
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(schema.kind.as_bytes());
        for (label, expr) in &schema.invariants {
            bytes.push(b'v');
            bytes.extend_from_slice(label.as_bytes());
            canonical_expr_bytes(expr, &mut Vec::new(), &mut bytes);
        }
    }
    let digest = emath_core::hash::sha256_digest(&bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok(Some(format!("emath:facts:v1:{hex}")))
}

/// Debug text with every `source: Span { .. }` field removed, so a type
/// renders the same wherever it sits in the file.
fn spanless(debug: &str) -> String {
    const FIELD: &str = "source: Span {";
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(at) = rest.find(FIELD) {
        out.push_str(&rest[..at]);
        let mut depth = 1usize;
        let mut end = rest.len();
        for (offset, ch) in rest[at + FIELD.len()..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = at + FIELD.len() + offset + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

impl Engine {
    /// Bind `inputs` to the entry function's parameters and evaluate it
    /// (absent inputs bind `Absent`, as `evaluate_function_at` does).
    pub(super) fn eval_entry(
        &mut self,
        name: &str,
        inputs: &BTreeMap<String, CValue>,
    ) -> Result<CValue, ConstructorError> {
        let decl = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
        let args: Vec<Expr> = decl
            .inputs
            .iter()
            .map(|input| {
                let value = inputs.get(input).cloned().unwrap_or(CValue::Absent);
                self.env.insert(input.clone(), value);
                Expr {
                    kind: ExprKind::Path {
                        segments: vec![input.clone()],
                        generics: None,
                    },
                    source: emath_core::Span::default(),
                }
            })
            .collect();
        self.eval_fn(name, decl, &args)
    }
}

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
use super::{
    CHECKPOINT_ABI, CValue, ConstructorError, Engine, Expr, ExprKind, FnDecl, Item, ObjectSchema,
    PathBuf, QueryDecl, Rc, SyntaxTree,
};

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

/// A module with its imports resolved and loaded, ready to evaluate
/// many times: loading reads and parses every imported file, which a
/// host running one module per batch would otherwise repeat each batch.
/// It retains the declarations read at `prepare_module` time, not execution
/// state. Loading alone does not perform the full semantic session checks.
#[derive(Clone)]
pub struct PreparedModule {
    functions: BTreeMap<String, Rc<FnDecl>>,
    queries: BTreeMap<String, QueryDecl>,
    objects: BTreeMap<String, ObjectSchema>,
    name_sources: BTreeMap<String, PathBuf>,
    pub(super) tree: Rc<SyntaxTree>,
    pub(super) imports: Rc<super::scope::ImportGraph>,
    pub(super) roots: Rc<Vec<PathBuf>>,
    source_path: Option<PathBuf>,
    source_directory: Option<PathBuf>,
    pub(super) admitted: bool,
}

impl std::fmt::Debug for PreparedModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedModule")
            .field("functions", &self.functions.len())
            .finish_non_exhaustive()
    }
}

/// Resolve and load `tree`'s imports (relative to `source`).
pub fn prepare_module(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<PreparedModule, ConstructorError> {
    let mut engine = super::setup::empty_engine();
    super::setup::install_local_items(&mut engine, tree)?;
    let roots = super::setup::module_roots_for(source);
    let imports = super::scope::load_graph(tree, &roots, source)?;
    super::setup::install_import_graph(&mut engine, tree, &imports)?;
    Ok(PreparedModule {
        functions: engine.functions,
        queries: engine.queries,
        objects: engine.objects,
        name_sources: engine.name_sources,
        tree: Rc::new(tree.clone()),
        imports: Rc::new(imports),
        roots: Rc::new(roots),
        source_path: source.map(Path::to_path_buf),
        source_directory: std::env::current_dir().ok(),
        admitted: false,
    })
}

impl PreparedModule {
    pub(super) fn checkpoint_imports(&self) -> Result<super::CheckpointImports, ConstructorError> {
        let absolute = |path: &Path| {
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                self.source_directory.as_ref()
                    .ok_or_else(|| fault("incompatible_checkpoint", "cannot freeze relative import path without an original directory"))?
                    .join(path)
            };
            if path.to_str().is_none() {
                return Err(fault(
                    "incompatible_checkpoint",
                    "checkpoint import paths must be UTF-8",
                ));
            }
            Ok(path)
        };
        Ok(super::CheckpointImports {
            source_path: self.source_path.as_deref().map(absolute).transpose()?,
            roots: self.roots.iter()
                .map(|path| absolute(path))
                .collect::<Result<_, _>>()?,
            files: self.imports.modules.iter()
                .map(|module| Ok((absolute(&module.file)?, module.text.clone())))
                .collect::<Result<_, ConstructorError>>()?,
        })
    }

    /// The root syntax used to prepare this module.
    pub fn tree(&self) -> &SyntaxTree {
        &self.tree
    }

    /// Lowering context from the retained import graph, without source reads.
    pub fn merged_tree(&self) -> Result<SyntaxTree, ConstructorError> {
        super::setup::merged_tree_from_graph(&self.tree, &self.imports)
    }

    /// Complete scoped declaration graph executed by this snapshot.
    /// Unlike the lowering projection, this includes private and unselected
    /// helpers. Alias edges use the same install keys as the fresh engine;
    /// neither paths nor source files are reread.
    pub fn meaning_tree(&self) -> Result<(SyntaxTree, BTreeMap<String, String>), ConstructorError> {
        self.imports.refuse_local_clobber(&self.tree)?;
        let scoped = self.imports.scoped_modules()?;
        let mut items = self.imports.scoped_root(&self.tree)?;
        let mut aliases = BTreeMap::new();
        for (module, decls) in self.imports.modules.iter().zip(&scoped) {
            items.extend(
                decls
                    .iter()
                    .map(|(_, decl)| Item::Declaration(decl.clone())),
            );
            items.push(Item::Exports {
                names: decls
                    .iter()
                    .filter(|(name, _)| !module.is_private(name))
                    .map(|(_, decl)| decl.name.clone())
                    .collect(),
                source: module.tree.source,
            });
        }
        for use_item in self.imports.root_uses.iter().chain(
            self.imports
                .modules
                .iter()
                .flat_map(|module| module.uses.iter()),
        ) {
            let index = self
                .imports
                .modules
                .iter()
                .position(|module| module.file == use_item.file)
                .ok_or_else(|| fault("E-USE-ADMISSION", "missing retained import module"))?;
            let module = &self.imports.modules[index];
            for (name, decl) in &scoped[index] {
                if module.is_private(name) {
                    continue;
                }
                let selected = match &use_item.tree {
                    super::UseTree::All => true,
                    super::UseTree::Named(names) if names.is_empty() => true,
                    super::UseTree::Named(names) => {
                        names.iter().any(|(original, _)| original == name)
                    }
                };
                if !selected {
                    continue;
                }
                aliases.insert(
                    format!("{}.{}", use_item.path.join("."), name),
                    decl.name.clone(),
                );
                if let super::UseTree::Named(names) = &use_item.tree {
                    if let Some((_, Some(alias))) =
                        names.iter().find(|(original, _)| original == name)
                    {
                        aliases.insert(alias.clone(), decl.name.clone());
                    }
                }
            }
        }
        aliases.extend(self.imports.root_bindings(&self.tree)?);
        Ok((
            SyntaxTree {
                source: self.tree.source,
                items,
            },
            aliases,
        ))
    }

    /// Source bytes and trees read for this module's dependencies.
    pub fn imported_modules(&self) -> impl Iterator<Item = (&Path, &str, &SyntaxTree)> {
        self.imports
            .modules
            .iter()
            .map(|module| (module.file.as_path(), module.text.as_str(), &module.tree))
    }

    /// Resolve a root `use` from the checked graph, not the current filesystem.
    pub fn resolved_use(&self, path: &[String]) -> Option<(PathBuf, bool)> {
        let imported = self
            .imports
            .root_uses
            .iter()
            .find(|item| item.path == path)?;
        Some((
            imported.file.clone(),
            self.roots
                .iter()
                .any(|root| imported.file.starts_with(root)),
        ))
    }

    /// Check a derived tree or an imported file against these retained sources.
    /// This does not admit the derived tree; the callback must use sema.
    pub fn with_imports<T>(&self, body: impl FnOnce() -> T) -> T {
        let files = self
            .imports
            .modules
            .iter()
            .map(|module| (module.file.clone(), module.text.clone()))
            .collect();
        super::setup::with_module_files(self.roots.as_ref().clone(), files, body)
    }

    /// Files carried by an engine-backed artifact, from the same import snapshot.
    pub fn import_files(&self) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
        (
            self.roots.as_ref().clone(),
            self.imports
                .modules
                .iter()
                .map(|module| (module.file.clone(), module.text.clone()))
                .collect(),
        )
    }

    /// Conservative snapshot equality for a host pairing a session with a
    /// freshly checked export. Dependency comments also count as a change.
    pub fn same_imports(&self, other: &Self) -> bool {
        self.roots == other.roots
            && self.imports.modules.len() == other.imports.modules.len()
            && self
                .imports
                .modules
                .iter()
                .zip(&other.imports.modules)
                .all(|(left, right)| left.file == right.file && left.text == right.text)
    }

    /// Completed facts are keyed by the declarations this module executes.
    pub fn fact_identity(&self) -> Option<String> {
        fact_identity_loaded(&self.fresh_engine())
    }

    /// Constructor admission against these already-resolved declarations.
    /// Full semantic session checks remain the responsibility of `emath-sema`.
    pub fn admit(&mut self) -> Result<(), ConstructorError> {
        if !self.admitted {
            super::admit::admit_loaded(&self.fresh_engine(), &self.tree)?;
            self.admitted = true;
        }
        Ok(())
    }

    pub fn object_names(&self) -> BTreeSet<String> {
        self.objects.keys().cloned().collect()
    }

    pub fn has_query(&self, name: &str) -> bool {
        self.queries.contains_key(name)
    }

    /// The runnable constructor entry names: the file's OWN `emath
    /// function` and `emath query` declarations, sorted. Origins in
    /// `name_sources` are the empty path for the local tree and the
    /// resolved import file for imports (bare, qualified aliases, and
    /// mangled private helpers alike), so an entry is a name whose
    /// origin is the empty path.
    pub fn entry_names(&self) -> Vec<String> {
        self.functions
            .keys()
            .chain(self.queries.keys())
            .filter(|name| {
                self.name_sources
                    .get(*name)
                    .is_some_and(|origin| origin.as_os_str().is_empty())
            })
            .cloned()
            .collect()
    }

    /// Input declarations from the resolved callable, including import aliases.
    pub fn input_types(&self, name: &str) -> Option<BTreeMap<String, emath_core::tree::TypeExpr>> {
        let function = self.functions.get(name)?;
        Some(
            function
                .inputs
                .iter()
                .cloned()
                .zip(function.input_types.iter().cloned())
                .collect(),
        )
    }

    /// Only immutable declarations are shared. Environments, buffers allocated
    /// by evaluation, memo tables, counters and continuations start afresh.
    pub(super) fn fresh_engine(&self) -> Engine {
        let mut engine = super::setup::empty_engine();
        engine.functions = self.functions.clone();
        engine.queries = self.queries.clone();
        engine.objects = self.objects.clone();
        engine.name_sources = self.name_sources.clone();
        engine
    }

    pub fn evaluate_function(
        &self,
        name: &str,
        inputs: &BTreeMap<String, CValue>,
        work_limit: u64,
    ) -> Result<CValue, ConstructorError> {
        let mut engine = self.fresh_engine();
        engine.work_limit = work_limit;
        engine.facts_only_memo = true;
        engine.eval_entry(name, inputs)
    }
}

pub(super) fn is_fact_key(key: &str) -> bool {
    key.strip_prefix("call:")
        .is_some_and(|rest| rest.starts_with(FACT_PREFIX))
        && !key.contains(":body=")
}

/// Evaluate a named function with `carried` facts pre-loaded into the
/// call memo, under `work_limit` engine steps. The evaluation never
/// suspends, so only fact-function calls and all-scalar calls are
/// memoized, and only fact calls are returned as fresh. The outer error is a
/// setup failure (no evaluation ran); an evaluation fault is
/// `CarriedEvaluation::value`. A carried hit skips the call's work, so
/// a warm memo can finish under a limit a cold one exceeds: the limit
/// is a resource bound, not part of the result.
///
/// `carried` is lent to the engine for the evaluation (moved in, not
/// copied) and comes back holding the fresh facts too, so the cost per
/// evaluation follows what it computed, not the size of the store.
pub fn evaluate_function_carrying(
    module: &PreparedModule,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    carried: &mut BTreeMap<String, CValue>,
    work_limit: u64,
) -> Result<CarriedEvaluation, ConstructorError> {
    let mut engine = module.fresh_engine();
    engine.work_limit = work_limit;
    engine.facts_only_memo = true;
    engine.memo = std::mem::take(carried);
    let value = engine.eval_entry(name, inputs);
    let hits = engine.fact_hits.get();
    let mut memo = std::mem::take(&mut engine.memo);
    let mut fresh = Vec::new();
    for key in std::mem::take(&mut engine.memo_added) {
        if is_fact_key(&key) {
            if let Some(value) = memo.get(&key) {
                fresh.push((key, value.clone()));
            }
        } else {
            memo.remove(&key);
        }
    }
    fresh.sort_by(|a, b| a.0.cmp(&b.0));
    *carried = memo;
    Ok(CarriedEvaluation { value, fresh, hits })
}

/// The identity of the module's fact functions and everything they
/// depend on, as `emath:facts:v1:<sha256 hex>`; `None` when the module
/// declares no fact function.
pub fn fact_identity(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<Option<String>, ConstructorError> {
    Ok(prepare_module(tree, source)?.fact_identity())
}

fn fact_identity_loaded(engine: &Engine) -> Option<String> {
    let mut pending: Vec<String> = engine
        .functions
        .keys()
        .filter(|name| name.starts_with(FACT_PREFIX))
        .cloned()
        .collect();
    if pending.is_empty() {
        return None;
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
    Some(format!("emath:facts:v1:{hex}"))
}

/// Debug text with every `source: Span { .. }` field removed, so a type
/// renders the same wherever it sits in the file.
pub(super) fn spanless(debug: &str) -> String {
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

//! Module scoping for `use`.
//!
//! A function or query name declared by exactly one file keeps its bare
//! global key, so a program without clashes loads exactly as a flat
//! namespace would. A name declared by two or more files is scoped: each
//! imported module's copy installs under `name__<module label>` and that
//! module's bodies are rewritten to call it. Every other file sees the
//! name through its own `use` lines, in priority order: an explicit
//! selection or alias, then a whole import, then a transitive import,
//! then any loaded module. A tie within the winning tier is `E-NAME-022`,
//! raised only when the file actually references the name.
//!
//! `exports:` is a module's surface. A function or query the module
//! does not list is private: it always installs under its scoped key, so
//! it never clashes with another file's names, and any importer spelling
//! of it (bare, selected, or `module.name`) is `E-NAME-025`.
//!
//! Object names are types and stay global: two files declaring the same
//! object still refuse at install.
//!
//! Inside `quote(...)` only call heads are rewritten; a bare quoted name
//! is an atom (refusal codes, symbolic variables), not a reference.

use super::prelude::*;
use super::{BTreeMap, BTreeSet, ConstructorError, Declaration, Expr, ExprKind, Item, Path, PathBuf, StmtKind, SyntaxTree, UseTree};
use emath_core::tree::{CommandArgument, Stmt, Suite};

pub(super) struct ModUse {
    pub file: PathBuf,
    pub path: Vec<String>,
    pub tree: UseTree,
}

pub(super) struct Module {
    pub file: PathBuf,
    /// Key suffix for scoped names (`use` path joined by `_`).
    pub label: String,
    /// The `use` path as written, for diagnostics.
    display: String,
    pub tree: SyntaxTree,
    pub uses: Vec<ModUse>,
    /// Declared name -> declaration kind.
    names: BTreeMap<String, String>,
    /// Names listed in the module's `exports:` line.
    exports: BTreeSet<String>,
}

impl Module {
    fn declares_callable(&self, name: &str) -> bool {
        self.names
            .get(name)
            .is_some_and(|kind| kind == "function" || kind == "query")
    }

    fn exports_callable(&self, name: &str) -> bool {
        self.declares_callable(name) && self.exports.contains(name)
    }

    pub(super) fn is_private(&self, name: &str) -> bool {
        self.declares_callable(name) && !self.exports.contains(name)
    }

    fn surface(&self) -> String {
        if self.exports.is_empty() {
            format!("`{}` has no `exports:` line, so it exports nothing", self.display)
        } else {
            let names: Vec<&str> = self.exports.iter().map(String::as_str).collect();
            format!("`{}` exports: {}", self.display, names.join(", "))
        }
    }
}

/// Every module reachable from one root file, each loaded once, in
/// install order (a module's own imports before the module).
pub(super) struct ImportGraph {
    pub root_uses: Vec<ModUse>,
    pub modules: Vec<Module>,
    index: BTreeMap<PathBuf, usize>,
    clashing: BTreeSet<String>,
    /// Callables private to at least one module.
    private: BTreeSet<String>,
}

#[derive(Clone)]
enum Resolution {
    Key(String),
    Ambiguous(Vec<String>),
    /// Only private declarations answer; each entry describes one
    /// module's surface.
    Private(Vec<String>),
}

type ScopeMap = BTreeMap<String, Resolution>;

/// One imported module after scoping: (declared name, installable
/// declaration under its key with rewritten bodies).
pub(super) type ScopedModule = Vec<(String, Declaration)>;

pub(super) fn load_graph(
    tree: &SyntaxTree,
    roots: &[PathBuf],
    source: Option<&Path>,
) -> Result<ImportGraph, ConstructorError> {
    let mut graph = ImportGraph {
        root_uses: Vec::new(),
        modules: Vec::new(),
        index: BTreeMap::new(),
        clashing: BTreeSet::new(),
        private: BTreeSet::new(),
    };
    let mut visiting = BTreeSet::new();
    graph.root_uses = load_uses(&mut graph, tree, roots, source, &mut visiting)?;
    let mut origins: BTreeMap<&str, usize> = BTreeMap::new();
    for name in callable_names(tree) {
        *origins.entry(name).or_default() += 1;
    }
    for module in &graph.modules {
        for (name, kind) in &module.names {
            if kind == "function" || kind == "query" {
                *origins.entry(name.as_str()).or_default() += 1;
            }
        }
    }
    graph.clashing = origins
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name.to_string())
        .collect();
    graph.private = graph
        .modules
        .iter()
        .flat_map(|m| m.names.keys().filter(|n| m.is_private(n)).cloned())
        .collect();
    let all_uses = graph
        .root_uses
        .iter()
        .chain(graph.modules.iter().flat_map(|m| m.uses.iter()));
    for u in all_uses {
        let module = graph.module(&u.file);
        if let UseTree::Named(names) = &u.tree {
            if let Some((name, _)) = names.iter().find(|(name, _)| module.is_private(name)) {
                return Err(private_error(name, &[module.surface()], "a `use` selection"));
            }
        }
    }
    Ok(graph)
}

fn callable_names(tree: &SyntaxTree) -> impl Iterator<Item = &str> {
    tree.items.iter().filter_map(|item| match item {
        Item::Declaration(decl) if matches!(decl.as_kind.as_str(), "function" | "query") => {
            Some(decl.name.as_str())
        }
        _ => None,
    })
}

fn load_uses(
    graph: &mut ImportGraph,
    tree: &SyntaxTree,
    roots: &[PathBuf],
    source: Option<&Path>,
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<Vec<ModUse>, ConstructorError> {
    let package = package_of(tree);
    let mut uses = Vec::new();
    for item in &tree.items {
        let Item::Use {
            path,
            tree: use_tree,
            ..
        } = item
        else {
            continue;
        };
        let file = resolve_import(path, roots, source, package)?;
        uses.push(ModUse {
            file: file.clone(),
            path: path.clone(),
            tree: use_tree.clone(),
        });
        if visiting.contains(&file) {
            return Err(fault(
                "E-USE-ADMISSION",
                format!("cyclic import `{}`", path.join(".")),
            ));
        }
        if graph.index.contains_key(&file) {
            continue;
        }
        let text = std::fs::read_to_string(&file).map_err(|err| {
            fault(
                "E-USE-ADMISSION",
                format!("cannot read {}: {err}", file.display()),
            )
        })?;
        let (imported, diagnostics) = emath_syntax::parse_str(&text);
        if diagnostics.has_errors() {
            return Err(fault(
                "E-USE-ADMISSION",
                format!("cannot parse {}: {diagnostics:?}", file.display()),
            ));
        }
        super::admit::check_exports(&imported).map_err(|err| {
            fault(err.code.as_str(), format!("{}: {}", file.display(), err.message))
        })?;
        visiting.insert(file.clone());
        let nested = load_uses(graph, &imported, roots, Some(file.as_path()), visiting)?;
        visiting.remove(&file);
        let names = imported
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Declaration(decl) => Some((decl.name.clone(), decl.as_kind.clone())),
                _ => None,
            })
            .collect();
        let exports = imported
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Exports { names, .. } => Some(names.iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect();
        let base = path.join("_");
        let mut label = base.clone();
        let mut n = 2;
        while graph.modules.iter().any(|m| m.label == label) {
            label = format!("{base}_{n}");
            n += 1;
        }
        graph.index.insert(file.clone(), graph.modules.len());
        graph.modules.push(Module {
            file,
            label,
            display: path.join("."),
            tree: imported,
            uses: nested,
            names,
            exports,
        });
    }
    Ok(uses)
}

impl ImportGraph {
    pub(super) fn module(&self, file: &Path) -> &Module {
        &self.modules[self.index[file]]
    }

    fn key(&self, module: &Module, name: &str) -> String {
        if self.clashing.contains(name) || module.is_private(name) {
            format!("{name}__{}", module.label)
        } else {
            name.to_string()
        }
    }

    fn pick(&self, found: BTreeMap<String, String>) -> Option<Resolution> {
        match found.len() {
            0 => None,
            1 => found.into_keys().next().map(Resolution::Key),
            _ => Some(Resolution::Ambiguous(found.into_values().collect())),
        }
    }

    /// What `name` means to a file whose imports are `uses`, when the
    /// file does not declare it.
    fn resolve(&self, uses: &[ModUse], name: &str) -> Option<Resolution> {
        let mut explicit = BTreeMap::new();
        for u in uses {
            let module = self.module(&u.file);
            if let UseTree::Named(names) = &u.tree {
                for (orig, alias) in names {
                    if alias.as_deref().unwrap_or(orig) == name && module.exports_callable(orig) {
                        explicit.insert(self.key(module, orig), module.display.clone());
                    }
                }
            }
        }
        if !explicit.is_empty() {
            return self.pick(explicit);
        }
        // Whole imports first; then names a partial selection left
        // unselected (still visible: every declaration installs). A
        // name the file aliased away is not offered under its own name.
        let mut whole = BTreeMap::new();
        let mut unselected = BTreeMap::new();
        for u in uses {
            let module = self.module(&u.file);
            if !module.exports_callable(name) {
                continue;
            }
            match &u.tree {
                UseTree::Named(names) if !names.is_empty() => {
                    if !names.iter().any(|(orig, _)| orig == name) {
                        unselected.insert(self.key(module, name), module.display.clone());
                    }
                }
                _ => {
                    whole.insert(self.key(module, name), module.display.clone());
                }
            }
        }
        if !whole.is_empty() {
            return self.pick(whole);
        }
        if !unselected.is_empty() {
            return self.pick(unselected);
        }
        let mut seen = BTreeSet::new();
        let mut queue: Vec<&Path> = uses.iter().map(|u| u.file.as_path()).collect();
        let mut transitive = BTreeMap::new();
        while let Some(file) = queue.pop() {
            if !seen.insert(file) {
                continue;
            }
            let module = self.module(file);
            if module.exports_callable(name) {
                transitive.insert(self.key(module, name), module.display.clone());
            }
            queue.extend(module.uses.iter().map(|u| u.file.as_path()));
        }
        if !transitive.is_empty() {
            return self.pick(transitive);
        }
        let anywhere = self
            .modules
            .iter()
            .filter(|m| m.exports_callable(name))
            .map(|m| (self.key(m, name), m.display.clone()))
            .collect();
        if let Some(resolution) = self.pick(anywhere) {
            return Some(resolution);
        }
        let hidden: Vec<String> = self
            .modules
            .iter()
            .filter(|m| m.is_private(name))
            .map(Module::surface)
            .collect();
        (!hidden.is_empty()).then_some(Resolution::Private(hidden))
    }

    /// `module.name` spellings this file could write for a private
    /// declaration, with that module's surface.
    fn private_paths(&self, uses: &[ModUse]) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for u in uses {
            let module = self.module(&u.file);
            let prefix = u.path.join(".");
            for name in module.names.keys().filter(|n| module.is_private(n)) {
                out.insert(format!("{prefix}.{name}"), module.surface());
            }
        }
        out
    }

    fn scope_map(&self, own: &BTreeMap<String, String>, uses: &[ModUse], own_module: Option<&Module>) -> ScopeMap {
        let mut map = ScopeMap::new();
        for name in self.clashing.union(&self.private) {
            let declared = own
                .get(name)
                .is_some_and(|kind| kind == "function" || kind == "query");
            let resolution = match (declared, own_module) {
                (true, Some(module)) => Some(Resolution::Key(self.key(module, name))),
                (true, None) => None,
                (false, _) => self.resolve(uses, name),
            };
            if let Some(resolution) = resolution {
                map.insert(name.clone(), resolution);
            }
        }
        map
    }

    /// Every imported module's declarations under their keys, bodies
    /// rewritten to the module's own scope.
    pub(super) fn scoped_modules(&self) -> Result<Vec<ScopedModule>, ConstructorError> {
        let mut out = Vec::with_capacity(self.modules.len());
        for module in &self.modules {
            let map = self.scope_map(&module.names, &module.uses, Some(module));
            let paths = self.private_paths(&module.uses);
            let mut decls = Vec::new();
            for item in &module.tree.items {
                let Item::Declaration(decl) = item else {
                    continue;
                };
                let mut scoped = if map.is_empty() && paths.is_empty() {
                    decl.clone()
                } else {
                    rewrite_declaration(decl, &map, &paths)
                        .map_err(|refusal| refusal.error(&format!("`{}`", module.display)))?
                };
                if matches!(decl.as_kind.as_str(), "function" | "query") {
                    scoped.name = self.key(module, &decl.name);
                }
                decls.push((decl.name.clone(), scoped));
            }
            out.push(decls);
        }
        Ok(out)
    }

    /// Clashing names the root file can reach, with what each means.
    fn root_map(&self, tree: &SyntaxTree) -> ScopeMap {
        let own = tree
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Declaration(decl) => Some((decl.name.clone(), decl.as_kind.clone())),
                _ => None,
            })
            .collect();
        self.scope_map(&own, &self.root_uses, None)
    }

    /// Root bare-name bindings for scoped names, after refusing any root
    /// reference that two imports answer equally.
    pub(super) fn root_bindings(&self, tree: &SyntaxTree) -> Result<Vec<(String, String)>, ConstructorError> {
        let map = self.root_map(tree);
        let paths = self.private_paths(&self.root_uses);
        for item in &tree.items {
            if let Item::Declaration(decl) = item {
                rewrite_declaration(decl, &map, &paths).map_err(|refusal| refusal.error("this file"))?;
            }
        }
        Ok(map
            .into_iter()
            .filter_map(|(name, resolution)| match resolution {
                Resolution::Key(key) if key != name => Some((name, key)),
                _ => None,
            })
            .collect())
    }

    /// The root file's declarations rewritten to its scope (build lane:
    /// the merged tree has no bare-name bindings).
    pub(super) fn scoped_root(&self, tree: &SyntaxTree) -> Result<Vec<Item>, ConstructorError> {
        let map = self.root_map(tree);
        let paths = self.private_paths(&self.root_uses);
        tree.items
            .iter()
            .map(|item| match item {
                Item::Declaration(decl) if !map.is_empty() || !paths.is_empty() => {
                    rewrite_declaration(decl, &map, &paths)
                        .map(Item::Declaration)
                        .map_err(|refusal| refusal.error("this file"))
                }
                other => Ok(other.clone()),
            })
            .collect()
    }

    /// E-NAME-022: a local declaration may not share a name with a
    /// declaration the file imports directly (whole module, or named in
    /// a selection). Helpers the file does not select stay scoped. The
    /// rule holds for every loaded module, not only the root: an
    /// imported module that redefines a name it imports would otherwise
    /// be scoped so its own bodies call the wrong one.
    pub(super) fn refuse_local_clobber(&self, tree: &SyntaxTree) -> Result<(), ConstructorError> {
        self.refuse_clobber_in(tree, &self.root_uses, "this file")?;
        self.modules.iter().try_for_each(|module| {
            self.refuse_clobber_in(&module.tree, &module.uses, &module.file.display().to_string())
        })
    }

    fn refuse_clobber_in(&self, tree: &SyntaxTree, uses: &[ModUse], at: &str) -> Result<(), ConstructorError> {
        let locals: BTreeSet<&str> = tree
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Declaration(decl) => Some(decl.name.as_str()),
                _ => None,
            })
            .collect();
        for u in uses {
            let module = self.module(&u.file);
            let visible: Vec<&str> = match &u.tree {
                UseTree::Named(names) if !names.is_empty() => names
                    .iter()
                    .map(|(orig, alias)| alias.as_deref().unwrap_or(orig))
                    .collect(),
                _ => module
                    .names
                    .keys()
                    .filter(|name| !module.is_private(name))
                    .map(String::as_str)
                    .collect(),
            };
            if let Some(name) = visible.into_iter().find(|name| locals.contains(name)) {
                return Err(fault(
                    "E-NAME-022",
                    format!(
                        "import of `{}` installs `{name}`, but a local declaration `{name}` already exists in {at}: \
                         an import may not replace a local name — rename one or import under an alias",
                        module.file.display()
                    ),
                ));
            }
        }
        Ok(())
    }
}

fn ambiguous(name: &str, labels: &[String], at: &str) -> ConstructorError {
    let modules: Vec<String> = labels.iter().map(|l| format!("`{l}`")).collect();
    fault(
        "E-NAME-022",
        format!(
            "`{name}` in {at} is declared by {}: select the one you mean, e.g. `use <module>.{{{name}}}` or `use <module>.{{{name} as <alias>}}`",
            modules.join(" and ")
        ),
    )
}

fn private_error(name: &str, surfaces: &[String], at: &str) -> ConstructorError {
    fault(
        "E-NAME-025",
        format!(
            "`{name}` in {at} is private: it is not in its module's `exports:` line ({}). Call an exported name, or add `{name}` to that module's `exports:`",
            surfaces.join("; ")
        ),
    )
}

enum Refusal {
    Ambiguous(String, Vec<String>),
    Private(String, Vec<String>),
}

impl Refusal {
    fn error(&self, at: &str) -> ConstructorError {
        match self {
            Self::Ambiguous(name, labels) => ambiguous(name, labels, at),
            Self::Private(name, surfaces) => private_error(name, surfaces, at),
        }
    }
}

struct Rewriter<'a> {
    map: &'a ScopeMap,
    private_paths: &'a BTreeMap<String, String>,
    bound: Vec<String>,
    quoted: bool,
    refusal: Option<Refusal>,
}

fn rewrite_declaration(
    decl: &Declaration,
    map: &ScopeMap,
    private_paths: &BTreeMap<String, String>,
) -> Result<Declaration, Refusal> {
    let mut rewriter = Rewriter {
        map,
        private_paths,
        bound: declaration_locals(decl),
        quoted: false,
        refusal: None,
    };
    let mut out = decl.clone();
    out.body = decl.body.iter().map(|stmt| rewriter.stmt(stmt)).collect();
    match rewriter.refusal {
        Some(refusal) => Err(refusal),
        None => Ok(out),
    }
}

/// Names a declaration binds for its whole body: fields, definitions,
/// lets, and test givens shadow module-level names.
fn declaration_locals(decl: &Declaration) -> Vec<String> {
    fn walk(stmts: &[Stmt], out: &mut Vec<String>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Section(section) => walk(&section.suite.statements, out),
                StmtKind::FieldDecl { name, .. } | StmtKind::Let { name, .. } | StmtKind::Given { name, .. } => {
                    out.push(name.clone());
                }
                StmtKind::Assign { target, .. } => out.extend(target.segments.first().cloned()),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&decl.body, &mut out);
    if let Some(signature) = &decl.signature {
        out.extend(signature.params.iter().map(|p| p.name.clone()));
    }
    out
}

impl Rewriter<'_> {
    fn lookup(&mut self, name: &str) -> Option<String> {
        if self.bound.iter().any(|b| b == name) {
            return None;
        }
        match self.map.get(name)? {
            Resolution::Key(key) => (key != name).then(|| key.clone()),
            Resolution::Ambiguous(labels) => {
                self.refusal
                    .get_or_insert_with(|| Refusal::Ambiguous(name.to_string(), labels.clone()));
                None
            }
            Resolution::Private(surfaces) => {
                self.refusal
                    .get_or_insert_with(|| Refusal::Private(name.to_string(), surfaces.clone()));
                None
            }
        }
    }

    fn check_qualified(&mut self, segments: &[String]) {
        if segments.len() < 2 || self.quoted {
            return;
        }
        let path = segments.join(".");
        if let Some(surface) = self.private_paths.get(&path) {
            self.refusal
                .get_or_insert_with(|| Refusal::Private(path.clone(), vec![surface.clone()]));
        }
    }

    fn under<T>(&mut self, name: &str, f: impl FnOnce(&mut Self) -> T) -> T {
        self.bound.push(name.to_string());
        let out = f(self);
        self.bound.pop();
        out
    }

    fn suite(&mut self, suite: &Suite) -> Suite {
        Suite {
            statements: suite.statements.iter().map(|s| self.stmt(s)).collect(),
            source: suite.source,
        }
    }

    fn stmt(&mut self, stmt: &Stmt) -> Stmt {
        let kind = match &stmt.kind {
            StmtKind::Section(section) => {
                let mut section = section.clone();
                section.suite = self.suite(&section.suite);
                StmtKind::Section(section)
            }
            StmtKind::FieldDecl { visibility, name, ty, default } => StmtKind::FieldDecl {
                visibility: *visibility,
                name: name.clone(),
                ty: ty.clone(),
                default: default.as_ref().map(|e| self.expr(e)),
            },
            StmtKind::FnDecl { visibility, head, name, params, ret, suite, source } => {
                let depth = self.bound.len();
                self.bound.extend(params.iter().map(|p| p.name.clone()));
                let suite = suite.as_ref().map(|s| self.suite(s));
                self.bound.truncate(depth);
                StmtKind::FnDecl {
                    visibility: *visibility,
                    head: head.clone(),
                    name: name.clone(),
                    params: params.clone(),
                    ret: ret.clone(),
                    suite,
                    source: *source,
                }
            }
            StmtKind::Let { name, ty, value } => StmtKind::Let {
                name: name.clone(),
                ty: ty.clone(),
                value: self.expr(value),
            },
            StmtKind::Assign { target, value } => {
                let mut target = target.clone();
                target.indices = target.indices.iter().map(|e| self.expr(e)).collect();
                StmtKind::Assign { target, value: self.expr(value) }
            }
            StmtKind::Require(e) => StmtKind::Require(self.expr(e)),
            StmtKind::Ensure(e) => StmtKind::Ensure(self.expr(e)),
            StmtKind::Invariant(e) => StmtKind::Invariant(self.expr(e)),
            StmtKind::Given { name, value } => StmtKind::Given {
                name: name.clone(),
                value: self.expr(value),
            },
            StmtKind::Expect(e) => StmtKind::Expect(self.expr(e)),
            StmtKind::Expr(e) => StmtKind::Expr(self.expr(e)),
            StmtKind::If { condition, then, else_branches, else_tail } => StmtKind::If {
                condition: self.expr(condition),
                then: self.suite(then),
                else_branches: else_branches
                    .iter()
                    .map(|(c, s)| (self.expr(c), self.suite(s)))
                    .collect(),
                else_tail: else_tail.as_ref().map(|s| self.suite(s)),
            },
            StmtKind::BinderStmt { kind, binders, suite, guard } => {
                let binders: Vec<_> = binders
                    .iter()
                    .map(|b| {
                        let mut b = b.clone();
                        b.domain = b.domain.as_ref().map(|d| self.expr(d));
                        b
                    })
                    .collect();
                let depth = self.bound.len();
                self.bound.extend(binders.iter().map(|b| b.name.clone()));
                let suite = self.suite(suite);
                let guard = guard.as_ref().map(|g| Box::new(self.expr(g)));
                self.bound.truncate(depth);
                StmtKind::BinderStmt { kind: *kind, binders, suite, guard }
            }
            StmtKind::SelfBlock { assignments } => StmtKind::SelfBlock {
                assignments: assignments
                    .iter()
                    .map(|(n, e)| (n.clone(), self.expr(e)))
                    .collect(),
            },
            StmtKind::Command { head, argument } => StmtKind::Command {
                head: head.clone(),
                argument: argument.as_ref().map(|arg| match arg {
                    CommandArgument::Expr(e) => CommandArgument::Expr(self.expr(e)),
                    CommandArgument::Assignment { name, value } => CommandArgument::Assignment {
                        name: name.clone(),
                        value: self.expr(value),
                    },
                    #[allow(unreachable_patterns)]
                    other => other.clone(),
                }),
            },
            StmtKind::Equation { left, right } => StmtKind::Equation {
                left: self.expr(left),
                right: self.expr(right),
            },
            other => other.clone(),
        };
        Stmt { kind, source: stmt.source }
    }

    fn boxed(&mut self, e: &Expr) -> Box<Expr> {
        Box::new(self.expr(e))
    }

    fn exprs(&mut self, es: &[Expr]) -> Vec<Expr> {
        es.iter().map(|e| self.expr(e)).collect()
    }

    fn quoted<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let was = std::mem::replace(&mut self.quoted, true);
        let out = f(self);
        self.quoted = was;
        out
    }

    fn head(&mut self, function: &Expr) -> Expr {
        if let ExprKind::Path { segments, generics } = &function.kind {
            if let [name] = segments.as_slice() {
                if let Some(key) = self.lookup(name) {
                    return Expr {
                        kind: ExprKind::Path { segments: vec![key], generics: generics.clone() },
                        source: function.source,
                    };
                }
                return function.clone();
            }
        }
        self.expr(function)
    }

    fn expr(&mut self, e: &Expr) -> Expr {
        let kind = match &e.kind {
            ExprKind::Path { segments, generics } => match segments.as_slice() {
                [name] if !self.quoted => match self.lookup(name) {
                    Some(key) => ExprKind::Path { segments: vec![key], generics: generics.clone() },
                    None => e.kind.clone(),
                },
                _ => {
                    self.check_qualified(segments);
                    e.kind.clone()
                }
            },
            ExprKind::Call { function, args } => ExprKind::Call {
                function: Box::new(self.head(function)),
                args: self.exprs(args),
            },
            ExprKind::Index { value, indices } => ExprKind::Index {
                value: self.boxed(value),
                indices: self.exprs(indices),
            },
            ExprKind::Slice { start, end } => ExprKind::Slice {
                start: start.as_ref().map(|x| self.boxed(x)),
                end: end.as_ref().map(|x| self.boxed(x)),
            },
            ExprKind::Unary { op, value } => ExprKind::Unary { op: *op, value: self.boxed(value) },
            ExprKind::Binary { op, left, right } => ExprKind::Binary {
                op: *op,
                left: self.boxed(left),
                right: self.boxed(right),
            },
            ExprKind::Approx { left, right, tolerance } => ExprKind::Approx {
                left: self.boxed(left),
                right: self.boxed(right),
                tolerance: tolerance.clone(),
            },
            ExprKind::If { condition, then_value, else_value } => ExprKind::If {
                condition: self.boxed(condition),
                then_value: self.boxed(then_value),
                else_value: self.boxed(else_value),
            },
            ExprKind::List(items) => ExprKind::List(self.exprs(items)),
            ExprKind::Set(items) => ExprKind::Set(self.exprs(items)),
            ExprKind::Tuple(items) => ExprKind::Tuple(self.exprs(items)),
            ExprKind::Table { headers, rows } => ExprKind::Table {
                headers: headers.clone(),
                rows: rows.iter().map(|r| self.exprs(r)).collect(),
            },
            ExprKind::SetComprehension { element, var, domain, guard } => {
                let domain = self.boxed(domain);
                let (element, guard) = self.under(var, |s| {
                    (s.boxed(element), guard.as_ref().map(|g| s.boxed(g)))
                });
                ExprKind::SetComprehension { element, var: var.clone(), domain, guard }
            }
            ExprKind::Record { type_path, fields } => ExprKind::Record {
                type_path: type_path.clone(),
                fields: fields.iter().map(|(n, v)| (n.clone(), self.expr(v))).collect(),
            },
            ExprKind::WithSeriesPolicy { value, interpolation, extrapolation } => ExprKind::WithSeriesPolicy {
                value: self.boxed(value),
                interpolation: *interpolation,
                extrapolation: *extrapolation,
            },
            ExprKind::Range { start, end, inclusive } => ExprKind::Range {
                start: start.as_ref().map(|x| self.boxed(x)),
                end: end.as_ref().map(|x| self.boxed(x)),
                inclusive: *inclusive,
            },
            ExprKind::Conditioned { value, condition } => ExprKind::Conditioned {
                value: self.boxed(value),
                condition: self.boxed(condition),
            },
            ExprKind::UnitQuery { kind, expr } => ExprKind::UnitQuery { kind: *kind, expr: self.boxed(expr) },
            ExprKind::Quantity { value, unit } => ExprKind::Quantity { value: self.boxed(value), unit: unit.clone() },
            ExprKind::Cases { subject, arms, else_arm } => ExprKind::Cases {
                subject: subject.as_ref().map(|x| self.boxed(x)),
                arms: arms.iter().map(|(c, v)| (self.expr(c), self.expr(v))).collect(),
                else_arm: self.boxed(else_arm),
            },
            ExprKind::FunctionAbs { param, domain, body } => ExprKind::FunctionAbs {
                param: param.clone(),
                domain: self.boxed(domain),
                body: self.under(param, |s| s.boxed(body)),
            },
            ExprKind::Recur { name, ty, body } => ExprKind::Recur {
                name: name.clone(),
                ty: self.boxed(ty),
                body: self.under(name, |s| s.boxed(body)),
            },
            ExprKind::Quote { body } => ExprKind::Quote { body: self.quoted(|s| s.boxed(body)) },
            ExprKind::QuoteBind { param, domain, body } => ExprKind::QuoteBind {
                param: param.clone(),
                domain: self.boxed(domain),
                body: self.under(param, |s| s.quoted(|s| s.boxed(body))),
            },
            ExprKind::CallableBinder { callee, param, domain, body } => ExprKind::CallableBinder {
                callee: Box::new(self.head(callee)),
                param: param.clone(),
                domain: self.boxed(domain),
                body: self.under(param, |s| s.boxed(body)),
            },
            ExprKind::SequenceCons {
                head,
                tail,
                spread_head,
            } => ExprKind::SequenceCons {
                head: self.boxed(head),
                tail: self.boxed(tail),
                spread_head: *spread_head,
            },
            other => other.clone(),
        };
        Expr { kind, source: e.source }
    }
}

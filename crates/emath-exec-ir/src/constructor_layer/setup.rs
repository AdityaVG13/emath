use super::prelude::*;
use super::scope;
use super::{SyntaxTree, Engine, ConstructorError, Path, BTreeSet, Environment, BTreeMap, HashMap, DEFAULT_WORK, Cell, Item, Declaration, Rc, FnDecl, QueryDecl, ObjectSchema, PathBuf, UseTree};

/// The module files a built artifact carries: the search roots and every
/// imported file (absolute path and text) as resolved when it was built.
struct EmbeddedModules {
    roots: Vec<PathBuf>,
    files: BTreeMap<PathBuf, String>,
}

thread_local! {
    static EMBEDDED: std::cell::RefCell<Option<Rc<EmbeddedModules>>> =
        const { std::cell::RefCell::new(None) };
}

fn embedded() -> Option<Rc<EmbeddedModules>> {
    EMBEDDED.with(|cell| cell.borrow().clone())
}

/// Run `body` with imports resolved only from the given roots and files,
/// never from disk or `EMATH_ROOT`: the same resolution the build saw,
/// so an artifact answers the same wherever it runs.
pub fn with_embedded_modules<T>(
    roots: &[&str],
    files: &[(&str, &str)],
    body: impl FnOnce() -> T,
) -> T {
    with_module_files(
        roots.iter().map(PathBuf::from).collect(),
        files
            .iter()
            .map(|(path, text)| (PathBuf::from(path), (*text).to_string()))
            .collect(),
        body,
    )
}

pub(super) fn with_module_files<T>(
    roots: Vec<PathBuf>,
    files: BTreeMap<PathBuf, String>,
    body: impl FnOnce() -> T,
) -> T {
    let modules = EmbeddedModules { roots, files };
    let previous = EMBEDDED.with(|cell| cell.replace(Some(Rc::new(modules))));
    struct Restore(Option<Rc<EmbeddedModules>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            EMBEDDED.with(|cell| *cell.borrow_mut() = previous);
        }
    }
    let _restore = Restore(previous);
    body()
}

/// The search roots and imported files (absolute path, text) a module
/// resolves, for an artifact to carry (see `with_embedded_modules`).
pub fn import_files(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<(Vec<PathBuf>, Vec<(PathBuf, String)>), ConstructorError> {
    let roots = module_roots_for(source);
    let graph = scope::load_graph(tree, &roots, source)?;
    let mut files = Vec::new();
    for module in &graph.modules {
        let text = read_module_file(&module.file).map_err(|err| {
            fault("E-USE-ADMISSION", format!("cannot read {}: {err}", module.file.display()))
        })?;
        files.push((module.file.clone(), text));
    }
    Ok((roots, files))
}

fn module_file_exists(path: &Path) -> bool {
    match embedded() {
        Some(modules) => modules.files.contains_key(path),
        None => path.is_file(),
    }
}

pub(super) fn read_module_file(path: &Path) -> std::io::Result<String> {
    match embedded() {
        Some(modules) => modules.files.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "not carried by this artifact")
        }),
        None => std::fs::read_to_string(path),
    }
}

pub(super) fn engine_from_tree(tree: &SyntaxTree) -> Result<Engine, ConstructorError> {
    engine_from_tree_at(tree, None)
}

pub(super) fn engine_from_tree_at(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<Engine, ConstructorError> {
    let mut engine = empty_engine();
    install_local_items(&mut engine, tree)?;
    load_imports(
        &mut engine,
        tree,
        &module_roots_for(source),
        source,
    )?;
    Ok(engine)
}

pub(super) fn empty_engine() -> Engine {
    Engine {
        env: Environment::default(),
        functions: BTreeMap::new(),
        queries: BTreeMap::new(),
        objects: BTreeMap::new(),
        name_sources: BTreeMap::new(),
        work: 0,
        work_limit: DEFAULT_WORK,
        memo: BTreeMap::new(),
        visit: 0,
        source_id: String::new(),
        entry: String::new(),
        source_text: String::new(),
        inputs: BTreeMap::new(),
        call_depth: 0,
        next_ref: 0,
        frames: Vec::new(),
        scopes: BTreeSet::new(),
        resume_frames: Vec::new(),
        expect_atoms: Cell::new(false),
        fact_hits: Cell::new(0),
        facts_only_memo: false,
        memo_added: Vec::new(),
        scalar_memo: HashMap::default(),
        tail_move: None,
        swapped_unwind: false,
    }
}

pub(super) fn install_local_items(
    engine: &mut Engine,
    tree: &SyntaxTree,
) -> Result<(), ConstructorError> {
    admit_constructor_surface(tree)?;
    for item in &tree.items {
        if let Item::Declaration(decl) = item {
            install_declaration(engine, decl, None, None)?;
        }
    }
    Ok(())
}

pub(super) fn install_declaration(
    engine: &mut Engine,
    decl: &Declaration,
    alias: Option<String>,
    source: Option<&Path>,
) -> Result<(), ConstructorError> {
    let name = alias.unwrap_or_else(|| decl.name.clone());
    // E-NAME-022: an import may not replace a name a different
    // origin installed. Origins: the local tree (empty path) and the
    // resolved import file. A re-install from the SAME origin is
    // idempotent, the transitive diamond (`use numerics.experiment`
    // pulls exact.integers via both bounds and powers) installs one
    // file twice under the same bare name and must keep admitting.
    let origin = source.map(Path::to_path_buf).unwrap_or_default();
    if let Some(existing) = engine.name_sources.get(&name) {
        if *existing != origin {
            let detail = if existing.as_os_str().is_empty() {
                format!(
                    "import of `{}` installs `{}`, but a local declaration `{}` already exists: \
                     an import may not replace a local name; rename one or import under an alias",
                    origin.display(),
                    name,
                    name
                )
            } else {
                // Functions and queries are scoped per module
                // (scope.rs); what still meets here is a global name,
                // an object type, declared by two files.
                format!(
                    "import of `{}` installs `{}`, already installed from `{}`: \
                     object names are global types; rename one of them",
                    origin.display(),
                    name,
                    existing.display()
                )
            };
            return Err(fault("E-NAME-022", detail));
        }
    }
    engine.name_sources.insert(name.clone(), origin);
    match decl.as_kind.as_str() {
        "function" => {
            let outputs = section_fields(decl, "outputs");
            let typed_inputs = section_typed_fields(decl, "inputs");
            engine.functions.insert(
                name,
                Rc::new(FnDecl {
                    inputs: typed_inputs.iter().map(|(name, _)| name.clone()).collect(),
                    generics: super::admit::generic_names(decl),
                    input_types: typed_inputs.into_iter().map(|(_, ty)| ty).collect(),
                    output: outputs.first().cloned(),
                    output_types: section_typed_fields(decl, "outputs")
                        .into_iter()
                        .map(|(_, ty)| ty)
                        .collect(),
                    outputs,
                    defs: constructor_defs(decl),
                    opaque: function_is_opaque(decl),
                }),
            );
        }
        "query" => {
            engine.queries.insert(
                name,
                QueryDecl {
                    inputs: section_fields(decl, "inputs"),
                    defs: section_assigns(decl, "definitions"),
                    form: section_text(decl, "answer", "form").unwrap_or_else(|| "value".into()),
                    method: section_text(decl, "using", "method"),
                    accept: section_text(decl, "answer", "accept"),
                },
            );
        }
        "object" => {
            engine.objects.insert(
                name,
                ObjectSchema {
                    kind: object_representation_kind(decl),
                    invariants: section_assigns(decl, "invariants"),
                },
            );
        }
        "feature" => {
            return Err(fault(
                "E-KIND-GONE",
                "declaration kind `feature` is not a core kind",
            ));
        }
        other => {
            return Err(fault(
                "E-KIND-GONE",
                format!("declaration kind `{other}` is not a core kind"),
            ));
        }
    }
    Ok(())
}

pub(super) fn package_of(tree: &SyntaxTree) -> Option<&[String]> {
    tree.items.iter().find_map(|item| match item {
        Item::Package { path, .. } => Some(path.as_slice()),
        _ => None,
    })
}

pub(super) fn resolve_import(
    path: &[String],
    roots: &[PathBuf],
    source: Option<&Path>,
    package: Option<&[String]>,
) -> Result<PathBuf, ConstructorError> {
    if let (Some(pkg), Some(src)) = (package, source) {
        if path.len() > pkg.len() && path.starts_with(pkg) {
            let rel = PathBuf::from_iter(path[pkg.len()..].iter()).with_extension("emath");
            let dir = src.parent().unwrap_or_else(|| Path::new("."));
            let candidate = dir.join(rel);
            if module_file_exists(&candidate) {
                return Ok(candidate);
            }
            return Err(fault(
                "E-PKG-050",
                format!("unresolved file import `{}`", path.join(".")),
            ));
        }
    }
    // Library roots keep priority: an existing `language/modules` or
    // `language/domains` module always wins, so the seam can never
    // shadow the shipped library.
    if let Ok(file) = resolve_module_path(path, roots) {
        return Ok(file);
    }
    // Directory-relative seam: after the stdlib roots miss, resolve the
    // same package path against the importing file's own directory, so
    // sibling files import each other without package declarations.
    // Unresolvable paths still refuse E-USE-ADMISSION below.
    if let Some(src) = source {
        let rel = PathBuf::from_iter(path.iter()).with_extension("emath");
        let dir = src.parent().unwrap_or_else(|| Path::new("."));
        let candidate = dir.join(rel);
        if module_file_exists(&candidate) {
            return Ok(candidate);
        }
    }
    Err(fault(
        "E-USE-ADMISSION",
        format!("unbound import `{}`", path.join(".")),
    ))
}

pub(super) fn load_imports(
    engine: &mut Engine,
    tree: &SyntaxTree,
    roots: &[PathBuf],
    source: Option<&Path>,
) -> Result<(), ConstructorError> {
    let graph = scope::load_graph(tree, roots, source)?;
    install_import_graph(engine, tree, &graph)
}

pub(super) fn install_import_graph(
    engine: &mut Engine,
    tree: &SyntaxTree,
    graph: &scope::ImportGraph,
) -> Result<(), ConstructorError> {
    graph.refuse_local_clobber(tree)?;
    let scoped = graph.scoped_modules()?;
    for (module, decls) in graph.modules.iter().zip(&scoped) {
        for (_, decl) in decls {
            if decl.as_kind == "feature" {
                return Err(fault(
                    "E-KIND-GONE",
                    "declaration kind `feature` is not a core kind",
                ));
            }
            install_declaration(engine, decl, None, Some(module.file.as_path()))?;
        }
    }
    let all_uses = graph
        .root_uses
        .iter()
        .chain(graph.modules.iter().flat_map(|m| m.uses.iter()));
    for u in all_uses {
        let index = graph.modules.iter().position(|m| m.file == u.file).unwrap_or_default();
        install_use_names(engine, u, &graph.modules[index], &scoped[index])?;
    }
    for (name, key) in graph.root_bindings(tree)? {
        if engine.name_sources.contains_key(&name) {
            continue;
        }
        let Some(origin) = engine.name_sources.get(&key).cloned() else {
            continue;
        };
        if let Some(decl_fn) = engine.functions.get(&key).cloned() {
            engine.functions.insert(name.clone(), decl_fn);
        }
        if let Some(decl_q) = engine.queries.get(&key).cloned() {
            engine.queries.insert(name.clone(), decl_q);
        }
        engine.name_sources.insert(name, origin);
    }
    Ok(())
}

/// The qualified (`path.name`) and aliased names one `use` line adds.
fn install_use_names(
    engine: &mut Engine,
    u: &scope::ModUse,
    module: &scope::Module,
    decls: &scope::ScopedModule,
) -> Result<(), ConstructorError> {
    let prefix = u.path.join(".");
    for (name, decl) in decls {
        if module.is_private(name) {
            continue;
        }
        let selected = match &u.tree {
            UseTree::All => true,
            UseTree::Named(names) if names.is_empty() => true,
            UseTree::Named(names) => names.iter().any(|(orig, _)| orig == name),
        };
        if !selected {
            continue;
        }
        // The namespaced copy (`prefix.name`) cannot collide with a
        // user identifier (dotted), so it records provenance without
        // a refusal path.
        let prefixed = format!("{prefix}.{name}");
        engine.name_sources.insert(prefixed.clone(), u.file.clone());
        if let Some(decl_fn) = engine.functions.get(&decl.name).cloned() {
            engine.functions.insert(prefixed.clone(), decl_fn);
        }
        if let Some(decl_q) = engine.queries.get(&decl.name).cloned() {
            engine.queries.insert(prefixed, decl_q);
        }
        if let UseTree::Named(names) = &u.tree {
            if let Some((_, Some(alias))) = names.iter().find(|(orig, _)| orig == name) {
                install_declaration(engine, decl, Some(alias.clone()), Some(u.file.as_path()))?;
            }
        }
    }
    Ok(())
}

/// Resolve `use fold.reduce` against the `language/modules` then
/// `language/domains` search roots.
pub fn resolve_module_path(
    path: &[String],
    roots: &[PathBuf],
) -> Result<PathBuf, ConstructorError> {
    let mut segs: Vec<String> = path.to_vec();
    if segs.first().map(String::as_str) == Some("language") {
        segs.remove(0);
    }
    if segs.first().map(String::as_str) == Some("modules") {
        segs.remove(0);
    }
    let rel = PathBuf::from_iter(segs.iter()).with_extension("emath");
    for root in roots {
        let candidate = root.join(&rel);
        if module_file_exists(&candidate) {
            return Ok(candidate);
        }
    }
    Err(fault(
        "E-USE-ADMISSION",
        format!("unbound import `{}`", path.join(".")),
    ))
}

/// Search roots for ordinary modules.
///
/// A non-empty `EMATH_ROOT` is the only source: its `language/modules`
/// and `language/domains`, the same tree the CLI loads the image from.
/// Mixing in another tree would let a different library answer an
/// import than the one the consumer pointed at. Otherwise the roots
/// come from walking up from the source file, then the working
/// directory, then this crate, nearest first.
///
/// Every `language/modules` root (general tools) precedes every
/// `language/domains` root (objects of study and applications), so a
/// domain module can never shadow a tool of the same path.
pub fn module_roots_for(source: Option<&Path>) -> Vec<PathBuf> {
    if let Some(modules) = embedded() {
        return modules.roots.clone();
    }
    let mut modules = Vec::new();
    let mut domains = Vec::new();
    let mut add = |language: PathBuf| {
        let tools = language.join("modules");
        if tools.is_dir() && !modules.contains(&tools) {
            modules.push(tools);
            let domain_root = language.join("domains");
            if domain_root.is_dir() {
                domains.push(domain_root);
            }
        }
    };
    match std::env::var_os("EMATH_ROOT").filter(|root| !root.is_empty()) {
        Some(root) => add(PathBuf::from(root).join("language")),
        None => {
            let mut starts = Vec::new();
            if let Some(parent) = source.and_then(Path::parent) {
                starts.push(parent.to_path_buf());
            }
            if let Ok(cwd) = std::env::current_dir() {
                starts.push(cwd);
            }
            starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
            for start in starts {
                for dir in start.ancestors().take(12) {
                    add(dir.join("language"));
                }
            }
        }
    }
    modules.extend(domains);
    modules
}

pub(super) fn merged_tree_from_graph(
    tree: &SyntaxTree,
    graph: &scope::ImportGraph,
) -> Result<SyntaxTree, ConstructorError> {
    graph.refuse_local_clobber(tree)?;
    let scoped = graph.scoped_modules()?;
    let root_items = graph.scoped_root(tree)?;
    let mut items = Vec::new();
    // `installed` is the diamond law: a module reached through two
    // import paths merges once (the engine's single install).
    let mut installed = BTreeSet::new();
    let mut uses = graph.root_uses.iter();
    for (item, scoped_item) in tree.items.iter().zip(root_items) {
        if matches!(item, Item::Use { .. }) {
            if let Some(u) = uses.next() {
                merge_use(graph, &scoped, u, &mut installed, &mut items);
            }
            continue;
        }
        items.push(scoped_item);
    }
    Ok(SyntaxTree {
        source: tree.source,
        items,
    })
}

/// Expand one `use` line: the imported module's own imports merge first
/// (engine install order), then every implementation joins under its scoped
/// key. Root references were already checked and rewritten by the scope graph.
fn merge_use(
    graph: &scope::ImportGraph,
    scoped: &[scope::ScopedModule],
    u: &scope::ModUse,
    installed: &mut BTreeSet<PathBuf>,
    items: &mut Vec<Item>,
) {
    if !installed.insert(u.file.clone()) {
        return;
    }
    let index = graph.modules.iter().position(|m| m.file == u.file).unwrap_or_default();
    for nested in &graph.modules[index].uses {
        merge_use(graph, scoped, nested, installed, items);
    }
    for (_, decl) in &scoped[index] {
        items.push(Item::Declaration(decl.clone()));
    }
}

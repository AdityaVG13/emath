use super::prelude::*;
use super::{SyntaxTree, Engine, ConstructorError, Path, BTreeSet, Environment, BTreeMap, DEFAULT_WORK, Cell, Item, Declaration, Rc, FnDecl, QueryDecl, ObjectSchema, PathBuf, UseTree};

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
        &mut BTreeSet::new(),
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
    // idempotent — the transitive diamond (`use numerics.experiment`
    // pulls exact.integers via both bounds and powers) installs one
    // file twice under the same bare name and must keep admitting.
    let origin = source.map(Path::to_path_buf).unwrap_or_default();
    if let Some(existing) = engine.name_sources.get(&name) {
        if *existing != origin {
            let detail = if existing.as_os_str().is_empty() {
                format!(
                    "import of `{}` installs `{}`, but a local declaration `{}` already exists: \
                     an import may not replace a local name — rename one or import under an alias",
                    origin.display(),
                    name,
                    name
                )
            } else {
                format!(
                    "import of `{}` installs `{}`, already installed from `{}`: \
                     import one of them under an alias",
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
            if candidate.is_file() {
                return Ok(candidate);
            }
            return Err(fault(
                "E-PKG-050",
                format!("unresolved file import `{}`", path.join(".")),
            ));
        }
    }
    // Stdlib roots keep priority: an existing `language/modules` module
    // always wins, so the seam can never shadow the standard library.
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
        if candidate.is_file() {
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
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<(), ConstructorError> {
    let package = package_of(tree);
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
        if visiting.contains(&file) {
            return Err(fault(
                "E-USE-ADMISSION",
                format!("cyclic import `{}`", path.join(".")),
            ));
        }
        visiting.insert(file.clone());
        let text = std::fs::read_to_string(&file).map_err(|err| {
            fault(
                "E-USE-ADMISSION",
                format!("cannot read {}: {err}", file.display()),
            )
        })?;
        let (imported, diagnostics) = emath_syntax::parse_str(&text);
        if diagnostics.has_errors() {
            visiting.remove(&file);
            return Err(fault(
                "E-USE-ADMISSION",
                format!("cannot parse {}: {diagnostics:?}", file.display()),
            ));
        }
        load_imports(engine, &imported, roots, Some(file.as_path()), visiting)?;
        install_imported(engine, &imported, path, use_tree, file.as_path())?;
        visiting.remove(&file);
    }
    Ok(())
}

pub(super) fn install_imported(
    engine: &mut Engine,
    tree: &SyntaxTree,
    path: &[String],
    use_tree: &UseTree,
    file: &Path,
) -> Result<(), ConstructorError> {
    let prefix = path.join(".");
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if matches!(decl.as_kind.as_str(), "feature") {
            return Err(fault(
                "E-KIND-GONE",
                "declaration kind `feature` is not a core kind",
            ));
        }
        let selected = match use_tree {
            UseTree::All => true,
            UseTree::Named(names) if names.is_empty() => true,
            UseTree::Named(names) => names.iter().any(|(name, _)| name == &decl.name),
        };
        if !selected {
            // Still install under the original name so callees in the same
            // module can resolve private helpers. The bare-name install
            // runs through the E-NAME-022 collision law like every other.
            install_declaration(engine, decl, None, Some(file))?;
            continue;
        }
        install_declaration(engine, decl, None, Some(file))?;
        // The namespaced copy (`prefix.name`) cannot collide with a
        // user identifier (dotted), so it records provenance without
        // a refusal path.
        let prefixed = format!("{prefix}.{}", decl.name);
        engine
            .name_sources
            .insert(prefixed.clone(), file.to_path_buf());
        if let Some(decl_fn) = engine.functions.get(&decl.name).cloned() { engine.functions.insert(prefixed, decl_fn); }
        if let Some(decl_q) = engine.queries.get(&decl.name).cloned() { engine
                .queries
                .insert(format!("{prefix}.{}", decl.name), decl_q); }
        if let UseTree::Named(names) = use_tree {
            if let Some((_, Some(alias))) = names.iter().find(|(name, _)| name == &decl.name) {
                install_declaration(engine, decl, Some(alias.clone()), Some(file))?;
            }
        }
    }
    Ok(())
}

/// Resolve `use fold.reduce` against `language/modules` search roots.
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
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fault(
        "E-USE-ADMISSION",
        format!("unbound import `{}`", path.join(".")),
    ))
}

/// Search roots for ordinary modules: walk from a source file, cwd, and this crate.
pub fn module_roots_for(source: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut consider = |start: PathBuf| {
        let mut dir = start;
        for _ in 0..12 {
            let modules = dir.join("language").join("modules");
            if modules.is_dir() {
                roots.push(modules);
            }
            if !dir.pop() {
                break;
            }
        }
    };
    if let Some(path) = source {
        if let Some(parent) = path.parent() {
            consider(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        consider(cwd);
    }
    consider(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    roots.sort();
    roots.dedup();
    roots
}

/// Merge a file's `use` imports into one whole-program tree (the build
/// lane's lowering input). Imported declarations join under their bare
/// names exactly as the engine installs them, the use selector filters
/// what a named import contributes, and nested imports merge before
/// their parent's declarations (engine load order). `package` and
/// `use` items stay the main file's authority; search roots follow the
/// same discovery as the test lane (`module_roots_for`).
pub fn merged_tree_with_imports(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<SyntaxTree, ConstructorError> {
    let roots = module_roots_for(source);
    let mut items = Vec::new();
    let mut visiting = BTreeSet::new();
    // `installed` is the diamond law: a module reached through two
    // import paths merges once (the engine's same-origin idempotent
    // install), while `visiting` only guards cycles on the active
    // path.
    let mut installed = BTreeSet::new();
    merge_tree_items(
        tree,
        source,
        &roots,
        &mut visiting,
        &mut installed,
        &mut items,
    )?;
    Ok(SyntaxTree {
        source: tree.source,
        items,
    })
}

/// The main file's own items push in order; each `use` expands to its
/// selected imported declarations in place (engine install order).
fn merge_tree_items(
    tree: &SyntaxTree,
    source: Option<&Path>,
    roots: &[PathBuf],
    visiting: &mut BTreeSet<PathBuf>,
    installed: &mut BTreeSet<PathBuf>,
    items: &mut Vec<Item>,
) -> Result<(), ConstructorError> {
    let package = package_of(tree);
    for item in &tree.items {
        let Item::Use {
            path,
            tree: use_tree,
            ..
        } = item
        else {
            items.push(item.clone());
            continue;
        };
        merge_use(
            path,
            use_tree,
            source,
            package,
            roots,
            visiting,
            installed,
            items,
        )?;
    }
    Ok(())
}

/// Expand one `use` statement: the imported file's own nested `use`s
/// merge first (engine load order, unselected by this selector), then
/// this selector's chosen declarations join under their bare names.
fn merge_use(
    path: &[String],
    use_tree: &UseTree,
    source: Option<&Path>,
    package: Option<&[String]>,
    roots: &[PathBuf],
    visiting: &mut BTreeSet<PathBuf>,
    installed: &mut BTreeSet<PathBuf>,
    items: &mut Vec<Item>,
) -> Result<(), ConstructorError> {
    let file = resolve_import(path, roots, source, package)?;
    if !installed.insert(file.clone()) {
        return Ok(());
    }
    if visiting.contains(&file) {
        return Err(fault(
            "E-USE-ADMISSION",
            format!("cyclic import `{}`", path.join(".")),
        ));
    }
    visiting.insert(file.clone());
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
            format!("imported module `{}` has parse errors", path.join(".")),
        ));
    }
    for item in &imported.items {
        if let Item::Use {
            path: nested_path,
            tree: nested_use,
            ..
        } = item
        {
            merge_use(
                nested_path,
                nested_use,
                Some(file.as_path()),
                package_of(&imported),
                roots,
                visiting,
                installed,
                items,
            )?;
        }
    }
    for sibling_item in imported.items {
        let Item::Declaration(decl) = &sibling_item else {
            continue;
        };
        let selected = match use_tree {
            UseTree::All => true,
            UseTree::Named(names) if names.is_empty() => true,
            UseTree::Named(names) => names.iter().any(|(name, _)| name == &decl.name),
        };
        if selected {
            items.push(sibling_item);
        }
    }
    visiting.remove(&file);
    Ok(())
}

use super::*;
use super::prelude::*;

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
        env: BTreeMap::new(),
        functions: BTreeMap::new(),
        queries: BTreeMap::new(),
        objects: BTreeMap::new(),
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

pub(super) fn install_local_items(engine: &mut Engine, tree: &SyntaxTree) -> Result<(), ConstructorError> {
    admit_constructor_surface(tree)?;
    for item in &tree.items {
        if let Item::Declaration(decl) = item {
            install_declaration(engine, decl, None)?;
        }
    }
    Ok(())
}

pub(super) fn install_declaration(
    engine: &mut Engine,
    decl: &Declaration,
    alias: Option<String>,
) -> Result<(), ConstructorError> {
    let name = alias.unwrap_or_else(|| decl.name.clone());
    match decl.as_kind.as_str() {
        "function" => {
            let outputs = section_fields(decl, "outputs");
            let typed_inputs = section_typed_fields(decl, "inputs");
            engine.functions.insert(
                name,
                FnDecl {
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
                },
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
    resolve_module_path(path, roots)
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
        let Item::Use { path, tree: use_tree, .. } = item else {
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
        install_imported(engine, &imported, path, use_tree)?;
        visiting.remove(&file);
    }
    Ok(())
}

pub(super) fn install_imported(
    engine: &mut Engine,
    tree: &SyntaxTree,
    path: &[String],
    use_tree: &UseTree,
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
            // module can resolve private helpers.
            install_declaration(engine, decl, None)?;
            continue;
        }
        install_declaration(engine, decl, None)?;
        engine.functions.get(&decl.name).cloned().map(|decl_fn| {
            engine.functions.insert(format!("{prefix}.{}", decl.name), decl_fn);
        });
        engine.queries.get(&decl.name).cloned().map(|decl_q| {
            engine.queries.insert(format!("{prefix}.{}", decl.name), decl_q);
        });
        if let UseTree::Named(names) = use_tree {
            if let Some((_, Some(alias))) = names.iter().find(|(name, _)| name == &decl.name) {
                install_declaration(engine, decl, Some(alias.clone()))?;
            }
        }
    }
    Ok(())
}

/// Resolve `use fold.reduce` against `language/modules` search roots.
pub fn resolve_module_path(path: &[String], roots: &[PathBuf]) -> Result<PathBuf, ConstructorError> {
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


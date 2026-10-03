//! emath source to a Lean 4 project.
//!
//! The translated subset: functions over `Int`, `Rat`, `Bool`,
//! `sequence(T)` and function types, with `definitions:` as `let`s,
//! closures, `if`, arithmetic, comparisons, lists, indexing, calls of
//! the file's own functions, of sibling user files (`use`), and of the
//! library functions the embedded prelude (`Prelude.lean`) defines.
//! Every authored `example` becomes a theorem the Lean kernel proves by
//! evaluation (`decide +kernel`), so a wrong value is a Lean error.
//! Anything else is refused by name, with the declarations that depend
//! on it; the rest of the file still translates.
//!
//! Semantics: `/` is always exact rational division, and integer
//! literals are typed `Int` unless a typed neighbour fixes their type,
//! so Lean never computes in `Nat`. Lean's functions are total where
//! emath refuses (zero divisors, indices out of range), so a value
//! emath computes is the value Lean computes; the converse is not
//! claimed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use emath_core::tree::{
    BinaryOp, Declaration, Expr, ExprKind, GenericArg, Item, StmtKind, SyntaxTree, TypeExpr,
    TypeKind, UnaryOp, UseTree,
};
use emath_exec_ir::constructor_emir::section_typed_fields;

pub(crate) const PRELUDE: &str = include_str!("Prelude.lean");
pub(crate) const TOOLCHAIN: &str = "leanprover/lean4:v4.34.1\n";
pub(crate) const LAKEFILE: &str = "name = \"emath_export\"\n\
defaultTargets = [\"Emath\", \"Export\"]\n\
\n\
[[lean_lib]]\n\
name = \"Emath\"\n\
globs = [\"Emath.+\"]\n\
\n\
[[lean_lib]]\n\
name = \"Export\"\n\
globs = [\"Export.+\"]\n";

/// Library functions and builtins `Prelude.lean` defines, by emath name.
const PRELUDE_NAMES: &[&str] = &[
    "range", "tabulate", "fold", "sum", "product", "int_sum", "int_prod", "length", "int_quot",
    "int_rem", "int_pow", "pow_int", "rat_pow",
];
/// Prelude functions with a generic accumulator: a literal argument
/// keeps its `Int` type rather than taking one from elsewhere.
const GENERIC_PRELUDE: &[&str] = &["fold"];

const LEAN_KEYWORDS: &[&str] = &[
    "abbrev", "at", "attribute", "axiom", "break", "by", "calc", "catch", "class", "continue",
    "decreasing_by", "def", "deriving", "do", "else", "end", "example", "exists", "export",
    "extends", "finally", "for", "forall", "from", "fun", "have", "if", "import", "in", "include",
    "inductive", "instance", "lemma", "let", "local", "macro", "match", "mut", "namespace",
    "nomatch", "nofun", "noncomputable", "notation", "obtain", "omit", "opaque", "open", "partial",
    "private", "protected", "rec", "return", "section", "set_option", "show", "sorry", "structure",
    "suffices", "syntax", "termination_by", "then", "theorem", "try", "unless", "universe",
    "using", "variable", "where", "with", "Prop", "Sort", "Type",
];

/// One authored example as a theorem, for naming Lean's verdict.
pub(crate) struct ExampleSite {
    pub lean: String,
    pub first_line: usize,
    pub last_line: usize,
    pub what: String,
}

#[derive(Default)]
pub(crate) struct Export {
    /// (path relative to the output directory, contents).
    pub files: Vec<(String, String)>,
    pub refusals: Vec<String>,
    pub sources: usize,
    pub functions: usize,
    pub examples: usize,
    pub library_examples: usize,
    pub sites: Vec<ExampleSite>,
}

struct Source {
    rel: String,
    tree: SyntaxTree,
    /// (selection, file) for each sibling user file this one uses.
    local_uses: Vec<(UseTree, PathBuf)>,
    /// (dotted path, selection, file) for each library module.
    library_uses: Vec<(String, UseTree, PathBuf)>,
}

/// What a file's translation offers to files that `use` it.
struct Module {
    namespace: String,
    module: String,
    exports: Option<Vec<String>>,
    translated: BTreeSet<String>,
    functions: BTreeSet<String>,
}

#[derive(Clone)]
enum Target {
    Lean(String),
    Refused,
}

/// Name resolution for one file: its own functions, its sibling
/// imports, the library modules it uses, then the prelude.
struct Names {
    targets: BTreeMap<String, Target>,
    library: BTreeMap<String, String>,
    used_prelude: BTreeSet<String>,
}

impl Names {
    fn resolve(&mut self, name: &str) -> Result<String, String> {
        match self.targets.get(name) {
            Some(Target::Lean(lean)) => return Ok(lean.clone()),
            Some(Target::Refused) => return Err(format!("calls `{name}`, which is not translated")),
            None => {}
        }
        if PRELUDE_NAMES.contains(&name) {
            self.used_prelude.insert(name.to_string());
            return Ok(format!("Emath.{name}"));
        }
        match self.library.get(name) {
            Some(module) => Err(format!("`{name}` (from `{module}`) has no Lean translation")),
            None => Err(format!("`{name}` has no Lean translation")),
        }
    }
}

/// Translate `roots` (absolute `.emath` paths under `base`) and every
/// sibling user file they reach.
pub(crate) fn export(roots: &[PathBuf], base: &Path) -> Result<Export, String> {
    let mut sources: BTreeMap<PathBuf, Source> = BTreeMap::new();
    let mut order = Vec::new();
    let mut visiting = BTreeSet::new();
    for root in roots {
        load(root, base, &mut sources, &mut order, &mut visiting)?;
    }
    let mut libraries: BTreeMap<PathBuf, SyntaxTree> = BTreeMap::new();
    let mut modules: BTreeMap<PathBuf, Module> = BTreeMap::new();
    let mut export = Export {
        sources: order.len(),
        ..Export::default()
    };
    let mut used_prelude = BTreeSet::new();
    let mut library_files = BTreeSet::new();
    for path in &order {
        let source = &sources[path];
        for (_, _, file) in &source.library_uses {
            library_files.insert(file.clone());
            if !libraries.contains_key(file) {
                libraries.insert(file.clone(), parse_library(file)?);
            }
        }
        let (module, lean, refusals, used) =
            translate_file(source, &modules, &libraries, &mut export)?;
        export.refusals.extend(refusals);
        used_prelude.extend(used);
        export.files.push((lean_path(&source.rel), lean));
        modules.insert(path.clone(), module);
    }
    let checks = library_checks(&used_prelude, &library_files, &libraries, &mut export);
    export.files.insert(0, ("Emath/Checks.lean".to_string(), checks));
    export.files.insert(0, ("Emath/Prelude.lean".to_string(), PRELUDE.to_string()));
    export.files.insert(0, ("lean-toolchain".to_string(), TOOLCHAIN.to_string()));
    export.files.insert(0, ("lakefile.toml".to_string(), LAKEFILE.to_string()));
    Ok(export)
}

fn load(
    path: &Path,
    base: &Path,
    sources: &mut BTreeMap<PathBuf, Source>,
    order: &mut Vec<PathBuf>,
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    if sources.contains_key(path) {
        return Ok(());
    }
    if !visiting.insert(path.to_path_buf()) {
        return Err(format!("{}: cyclic import", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let (tree, diagnostics) = emath_syntax::parse_str(&text);
    if diagnostics.has_errors() {
        return Err(format!("{}: does not parse; run `emath check` on it", path.display()));
    }
    let rel = relative(path, base)
        .ok_or_else(|| format!("{} is outside the exported folder {}", path.display(), base.display()))?;
    let mut local_uses = Vec::new();
    let mut library_uses = Vec::new();
    for item in &tree.items {
        let Item::Use { path: use_path, tree: selection, .. } = item else {
            continue;
        };
        let (file, library) = emath_exec_ir::constructor_layer::resolve_use(&tree, use_path, path)
            .map_err(|error| format!("{rel}: {}: {}", error.code, error.message))?;
        if library {
            library_uses.push((use_path.join("."), selection.clone(), file));
        } else {
            let file = file.canonicalize().unwrap_or(file);
            load(&file, base, sources, order, visiting)?;
            local_uses.push((selection.clone(), file));
        }
    }
    visiting.remove(path);
    order.push(path.to_path_buf());
    sources.insert(path.to_path_buf(), Source { rel, tree, local_uses, library_uses });
    Ok(())
}

fn relative(path: &Path, base: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    let parts: Vec<String> = rel.iter().map(|part| part.to_string_lossy().to_string()).collect();
    Some(parts.join("/"))
}

fn parse_library(file: &Path) -> Result<SyntaxTree, String> {
    let text = std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let (tree, diagnostics) = emath_syntax::parse_str(&text);
    if diagnostics.has_errors() {
        return Err(format!("library module {} does not parse", file.display()));
    }
    Ok(tree)
}

fn functions(tree: &SyntaxTree) -> impl Iterator<Item = &Declaration> {
    tree.items.iter().filter_map(|item| match item {
        Item::Declaration(decl) if decl.as_kind == "function" => Some(decl),
        _ => None,
    })
}

fn exports(tree: &SyntaxTree) -> Option<Vec<String>> {
    tree.items.iter().find_map(|item| match item {
        Item::Exports { names, .. } => Some(names.clone()),
        _ => None,
    })
}

/// The (local name, defined name) pairs a `use` selection brings in.
fn selected(selection: &UseTree, available: impl Iterator<Item = String>) -> Vec<(String, String)> {
    match selection {
        // A bare `use m` parses as an empty selection: the whole surface.
        UseTree::All => available.map(|name| (name.clone(), name)).collect(),
        UseTree::Named(names) if names.is_empty() => available.map(|name| (name.clone(), name)).collect(),
        UseTree::Named(names) => names
            .iter()
            .map(|(name, alias)| (alias.clone().unwrap_or_else(|| name.clone()), name.clone()))
            .collect(),
    }
}

/// The Lean file for a source, under `Export/` at the same relative path.
fn lean_path(rel: &str) -> String {
    format!("Export/{}.lean", rel.strip_suffix(".emath").unwrap_or(rel))
}

fn lean_module(rel: &str) -> String {
    let stem = rel.strip_suffix(".emath").unwrap_or(rel);
    let mut parts = vec!["Export".to_string()];
    parts.extend(stem.split('/').map(ident));
    parts.join(".")
}

type FileTranslation = (Module, String, Vec<String>, BTreeSet<String>);

fn translate_file(
    source: &Source,
    modules: &BTreeMap<PathBuf, Module>,
    libraries: &BTreeMap<PathBuf, SyntaxTree>,
    export: &mut Export,
) -> Result<FileTranslation, String> {
    let module = lean_module(&source.rel);
    let mut names = Names {
        targets: BTreeMap::new(),
        library: BTreeMap::new(),
        used_prelude: BTreeSet::new(),
    };
    let mut imports = vec!["Emath.Prelude".to_string()];
    for (selection, file) in &source.local_uses {
        let imported = &modules[file];
        let available = imported
            .exports
            .clone()
            .unwrap_or_else(|| imported.functions.iter().cloned().collect());
        // Imported names stay qualified: an imported file's private
        // functions may share a name with this file's.
        for (local, name) in selected(selection, available.into_iter()) {
            let target = if imported.translated.contains(&name) {
                Target::Lean(format!("{}.{}", imported.namespace, ident(&name)))
            } else {
                Target::Refused
            };
            names.targets.insert(local, target);
        }
        imports.push(imported.module.clone());
    }
    for (display, selection, file) in &source.library_uses {
        let tree = &libraries[file];
        let available = functions(tree).map(|decl| decl.name.clone());
        for (local, _) in selected(selection, available) {
            names.library.insert(local, display.clone());
        }
    }

    let mut refusals = Vec::new();
    let mut body = String::new();
    let own: Vec<&Declaration> = functions(&source.tree).collect();
    let own_names: BTreeSet<String> = own.iter().map(|decl| decl.name.clone()).collect();
    for item in &source.tree.items {
        if let Item::Declaration(decl) = item
            && decl.as_kind != "function"
        {
            refusals.push(format!(
                "not translated: {} `{}`: an emath {} has no Lean translation yet",
                source.rel, decl.name, decl.as_kind
            ));
        }
    }
    let mut translated = BTreeSet::new();
    let order = ordered(&own, &own_names, &mut |decl, reason| {
        refusals.push(format!("not translated: {} `{}`: {reason}", source.rel, decl.name));
    });
    for decl in order {
        match function(decl, &mut names) {
            Ok(text) => {
                body.push_str(&text);
                names.targets.insert(decl.name.clone(), Target::Lean(ident(&decl.name)));
                translated.insert(decl.name.clone());
                export.functions += 1;
            }
            Err(reason) => {
                refusals.push(format!("not translated: {} `{}`: {reason}", source.rel, decl.name));
                names.targets.insert(decl.name.clone(), Target::Refused);
            }
        }
    }
    for name in &own_names {
        names.targets.entry(name.clone()).or_insert(Target::Refused);
    }

    let mut text = format!(
        "/-\nGenerated by `emath export lean` from {}; the next export\noverwrites it.\n-/\n",
        source.rel
    );
    for import in &imports {
        text.push_str(&format!("import {import}\n"));
    }
    // emath binders may go unused (a fold's index, say).
    text.push_str("\nset_option linter.unusedVariables false\n\n");
    text.push_str(&format!("namespace {module}\n\n"));
    let lean_path = lean_path(&source.rel);
    text.push_str(&body);
    for decl in &own {
        if !translated.contains(&decl.name) {
            continue;
        }
        let params = section_typed_fields(decl, "inputs");
        let output = section_typed_fields(decl, "outputs").remove(0).0;
        for case in examples(decl) {
            let what = format!("{} example <{}> of `{}`", source.rel, case.label, decl.name);
            match theorem(&decl.name, &ident(&decl.name), &params, &output, &case, &mut names) {
                Ok(theorem) => {
                    let first_line = text.lines().count() + 1;
                    text.push_str(&theorem);
                    export.sites.push(ExampleSite {
                        lean: lean_path.clone(),
                        first_line,
                        last_line: text.lines().count(),
                        what,
                    });
                    export.examples += 1;
                }
                Err(reason) => refusals.push(format!("not translated: {what}: {reason}")),
            }
        }
    }
    text.push_str(&format!("end {module}\n"));
    let namespace = module.clone();
    Ok((
        Module {
            namespace,
            module,
            exports: exports(&source.tree),
            translated,
            functions: own_names,
        },
        text,
        refusals,
        names.used_prelude,
    ))
}

/// The file's functions, callees first. A function on a call cycle
/// is reported through `refuse` and left out.
fn ordered<'a>(
    own: &[&'a Declaration],
    own_names: &BTreeSet<String>,
    refuse: &mut dyn FnMut(&Declaration, &str),
) -> Vec<&'a Declaration> {
    let by_name: BTreeMap<&str, &Declaration> = own.iter().map(|decl| (decl.name.as_str(), *decl)).collect();
    let mut done = BTreeSet::new();
    let mut cyclic = BTreeSet::new();
    let mut out = Vec::new();
    fn visit<'a>(
        decl: &'a Declaration,
        by_name: &BTreeMap<&str, &'a Declaration>,
        own_names: &BTreeSet<String>,
        stack: &mut Vec<String>,
        done: &mut BTreeSet<String>,
        cyclic: &mut BTreeSet<String>,
        out: &mut Vec<&'a Declaration>,
    ) {
        if done.contains(&decl.name) {
            return;
        }
        if let Some(at) = stack.iter().position(|name| *name == decl.name) {
            cyclic.extend(stack[at..].iter().cloned());
            return;
        }
        stack.push(decl.name.clone());
        let mut callees = BTreeSet::new();
        for stmt in &decl.body {
            collect_stmt_names(stmt, &mut callees);
        }
        for callee in callees.iter().filter(|name| own_names.contains(*name)) {
            visit(by_name[callee.as_str()], by_name, own_names, stack, done, cyclic, out);
        }
        stack.pop();
        done.insert(decl.name.clone());
        out.push(decl);
    }
    for decl in own {
        visit(decl, &by_name, own_names, &mut Vec::new(), &mut done, &mut cyclic, &mut out);
    }
    for decl in out.iter().filter(|decl| cyclic.contains(&decl.name)) {
        refuse(decl, "recursion has no Lean translation yet");
    }
    out.into_iter().filter(|decl| !cyclic.contains(&decl.name)).collect()
}

fn collect_stmt_names(stmt: &emath_core::tree::Stmt, names: &mut BTreeSet<String>) {
    match &stmt.kind {
        StmtKind::Section(section) => {
            if section.name == "tests" {
                return;
            }
            for inner in &section.suite.statements {
                collect_stmt_names(inner, names);
            }
        }
        StmtKind::Assign { value, .. } => collect_names(value, names),
        _ => {}
    }
}

fn collect_names(expr: &Expr, names: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Path { segments, .. } => {
            if let Some(first) = segments.first() {
                names.insert(first.clone());
            }
        }
        ExprKind::Call { function, args } => {
            collect_names(function, names);
            args.iter().for_each(|arg| collect_names(arg, names));
        }
        ExprKind::Index { value, indices } => {
            collect_names(value, names);
            indices.iter().for_each(|index| collect_names(index, names));
        }
        ExprKind::Unary { value, .. } => collect_names(value, names),
        ExprKind::Binary { left, right, .. } => {
            collect_names(left, names);
            collect_names(right, names);
        }
        ExprKind::If { condition, then_value, else_value } => {
            collect_names(condition, names);
            collect_names(then_value, names);
            collect_names(else_value, names);
        }
        ExprKind::List(items) => items.iter().for_each(|item| collect_names(item, names)),
        ExprKind::FunctionAbs { body, .. } => collect_names(body, names),
        ExprKind::SequenceCons { head, tail, .. } => {
            collect_names(head, names);
            collect_names(tail, names);
        }
        _ => {}
    }
}

fn function(decl: &Declaration, names: &mut Names) -> Result<String, String> {
    if !decl.generics.is_empty() {
        return Err("generic functions have no Lean translation yet".into());
    }
    for section in decl.sections() {
        if !matches!(section.name.as_str(), "inputs" | "outputs" | "definitions" | "tests") {
            return Err(format!("section `{}:` has no Lean translation", section.name));
        }
    }
    let params = section_typed_fields(decl, "inputs");
    let outputs = section_typed_fields(decl, "outputs");
    let [(output, output_ty)] = outputs.as_slice() else {
        return Err("only functions with one output translate".into());
    };
    let mut head = format!("def {}", ident(&decl.name));
    let mut scope = Vec::new();
    for (name, ty) in &params {
        head.push_str(&format!(" ({} : {})", ident(name), lean_type(ty)?));
        scope.push(name.clone());
    }
    head.push_str(&format!(" : {} :=\n", lean_type(output_ty)?));
    let mut ctx = Ctx { names, scope };
    let mut lines = String::new();
    let mut assigned = false;
    for section in decl.sections().filter(|section| section.name == "definitions") {
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::Assign { target, value } if target.segments.len() == 1 && target.indices.is_empty() => {
                    let name = &target.segments[0];
                    let (value, _) = ctx.expr(value, false)?;
                    lines.push_str(&format!("  let {} := {value}\n", ident(name)));
                    ctx.scope.push(name.clone());
                    assigned |= name == output;
                }
                StmtKind::Require(_) => {}
                _ => return Err("a definition that is not `name = value` has no Lean translation".into()),
            }
        }
    }
    if !assigned {
        return Err(format!("output `{output}` is never defined"));
    }
    Ok(format!("{head}{lines}  {}\n\n", ident(output)))
}

struct Case {
    label: String,
    givens: Vec<(String, Expr)>,
    expects: Vec<Expr>,
}

fn examples(decl: &Declaration) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    let mut labels = BTreeMap::<String, usize>::new();
    let mut loose = Case { label: decl.name.clone(), givens: Vec::new(), expects: Vec::new() };
    let mut push = |cases: &mut Vec<Case>, mut case: Case| {
        let seen = labels.entry(case.label.clone()).or_default();
        *seen += 1;
        if *seen > 1 {
            case.label = format!("{}_{}", case.label, seen);
        }
        cases.push(case);
    };
    for section in decl.sections().filter(|section| section.name == "tests") {
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::Section(example) if example.name == "example" => {
                    let mut case = Case {
                        label: example.generic.clone().unwrap_or_else(|| decl.name.clone()),
                        givens: Vec::new(),
                        expects: Vec::new(),
                    };
                    for inner in &example.suite.statements {
                        match &inner.kind {
                            StmtKind::Given { name, value } => case.givens.push((name.clone(), value.clone())),
                            StmtKind::Expect(expr) => case.expects.push(expr.clone()),
                            _ => {}
                        }
                    }
                    push(&mut cases, case);
                }
                StmtKind::Given { name, value } => loose.givens.push((name.clone(), value.clone())),
                StmtKind::Expect(expr) => loose.expects.push(expr.clone()),
                _ => {}
            }
        }
    }
    if !loose.expects.is_empty() {
        push(&mut cases, loose);
    }
    cases
}

/// `name.label : (let givens; let output := f args; expects) = true`,
/// proved by the kernel evaluating it.
fn theorem(
    name: &str,
    callee: &str,
    params: &[(String, TypeExpr)],
    output: &str,
    case: &Case,
    names: &mut Names,
) -> Result<String, String> {
    if case.expects.is_empty() {
        return Err("the example expects nothing".into());
    }
    let mut ctx = Ctx { names, scope: Vec::new() };
    let mut lines = Vec::new();
    let mut args = Vec::new();
    for (param, ty) in params {
        let (_, value) = case
            .givens
            .iter()
            .find(|(given, _)| given == param)
            .ok_or_else(|| format!("the example does not give `{param}`"))?;
        let (value, _) = ctx.expr(value, false)?;
        match lean_type(ty) {
            Ok(ty) => lines.push(format!("let {} : {ty} := {value}", ident(param))),
            Err(_) => lines.push(format!("let {} := {value}", ident(param))),
        }
        args.push(ident(param));
    }
    ctx.scope.extend(params.iter().map(|(param, _)| param.clone()));
    lines.push(format!("let {} := {callee}{}", ident(output), args.iter().map(|arg| format!(" {arg}")).collect::<String>()));
    ctx.scope.push(output.to_string());
    let mut checks = Vec::new();
    for expect in &case.expects {
        let (check, level) = ctx.expr(expect, false)?;
        checks.push(if level > LEVEL_AND { format!("({check})") } else { check });
    }
    lines.push(checks.join(" && "));
    let body = lines.join("\n    ");
    Ok(format!(
        "theorem {}.{} : (\n    {body}) = true := by\n  decide +kernel\n\n",
        ident(name),
        ident(&case.label)
    ))
}

/// `Emath/Checks.lean`: the library's own authored examples for every
/// prelude function the export used, run against the prelude.
fn library_checks(
    used: &BTreeSet<String>,
    files: &BTreeSet<PathBuf>,
    libraries: &BTreeMap<PathBuf, SyntaxTree>,
    export: &mut Export,
) -> String {
    let mut text = String::from(
        "/-\nGenerated by `emath export lean`: the emath library's authored examples\nfor the prelude functions this export uses, checked against the prelude.\n-/\nimport Emath.Prelude\n\nnamespace Emath.Checks\n\n",
    );
    for file in files {
        for decl in functions(&libraries[file]).filter(|decl| used.contains(&decl.name)) {
            let params = section_typed_fields(decl, "inputs");
            let mut outputs = section_typed_fields(decl, "outputs");
            if outputs.len() != 1 {
                continue;
            }
            let output = outputs.remove(0).0;
            for case in examples(decl) {
                let mut names = Names {
                    targets: BTreeMap::new(),
                    library: BTreeMap::new(),
                    used_prelude: BTreeSet::new(),
                };
                let callee = format!("Emath.{}", decl.name);
                if let Ok(theorem) = theorem(&decl.name, &callee, &params, &output, &case, &mut names) {
                    let first_line = text.lines().count() + 1;
                    text.push_str(&theorem);
                    export.sites.push(ExampleSite {
                        lean: "Emath/Checks.lean".into(),
                        first_line,
                        last_line: text.lines().count(),
                        what: format!("library example <{}> of `{}` against the prelude", case.label, decl.name),
                    });
                    export.library_examples += 1;
                }
            }
        }
    }
    text.push_str("end Emath.Checks\n");
    text
}

const LEVEL_ATOM: u8 = 0;
const LEVEL_APP: u8 = 1;
const LEVEL_MUL: u8 = 2;
const LEVEL_ADD: u8 = 3;
const LEVEL_CONS: u8 = 4;
const LEVEL_AND: u8 = 5;
const LEVEL_OR: u8 = 6;
const LEVEL_OPEN: u8 = 9;

struct Ctx<'a> {
    names: &'a mut Names,
    scope: Vec<String>,
}

impl Ctx<'_> {
    /// Lean text and its binding level. `bare`: an integer literal may
    /// stay untyped because a typed neighbour fixes its type.
    fn expr(&mut self, expr: &Expr, bare: bool) -> Result<(String, u8), String> {
        if !bare && is_literal(expr) && !has_division(expr) {
            let (text, _) = self.expr(expr, true)?;
            return Ok((format!("({text} : Int)"), LEVEL_ATOM));
        }
        match &expr.kind {
            ExprKind::Int(digits) => Ok((digits.replace('_', ""), LEVEL_ATOM)),
            ExprKind::Rational { numer, denom } => Ok((
                format!("(({} : Rat) / {})", numer.replace('_', ""), denom.replace('_', "")),
                LEVEL_ATOM,
            )),
            ExprKind::Bool(value) => Ok((value.to_string(), LEVEL_ATOM)),
            ExprKind::Path { segments, generics: None } => self.path(segments),
            ExprKind::Call { function, args } => self.call(function, args),
            ExprKind::Index { value, indices } => {
                let [index] = indices.as_slice() else {
                    return Err("indexing with several indices has no Lean translation".into());
                };
                let value = self.operand(value, false, LEVEL_ATOM)?;
                let index = self.operand(index, true, LEVEL_ATOM)?;
                Ok((format!("Emath.index {value} {index}"), LEVEL_APP))
            }
            ExprKind::Unary { op: UnaryOp::Pos, value } => self.expr(value, bare),
            ExprKind::Unary { op: UnaryOp::Neg, value } => {
                let value = self.operand(value, bare || is_literal(value), LEVEL_ATOM)?;
                Ok((format!("-{value}"), LEVEL_APP))
            }
            ExprKind::Unary { op: UnaryOp::Not, value } => {
                let value = self.operand(value, false, LEVEL_ATOM)?;
                Ok((format!("!{value}"), LEVEL_APP))
            }
            ExprKind::Binary { op, left, right } => self.binary(*op, left, right, bare),
            ExprKind::If { condition, then_value, else_value } => {
                let (condition, _) = self.expr(condition, false)?;
                let then_bare = is_literal(then_value) && !is_literal(else_value);
                let else_bare = is_literal(else_value) && !is_literal(then_value);
                let (then_value, _) = self.expr(then_value, then_bare)?;
                let (else_value, _) = self.expr(else_value, else_bare)?;
                Ok((format!("if {condition} then {then_value} else {else_value}"), LEVEL_OPEN))
            }
            ExprKind::List(items) => {
                let all_literal = items.iter().all(is_literal);
                let mut parts = Vec::new();
                for item in items {
                    parts.push(self.expr(item, !all_literal && is_literal(item))?.0);
                }
                Ok((format!("[{}]", parts.join(", ")), LEVEL_ATOM))
            }
            ExprKind::SequenceCons { head, tail, spread_head } => {
                let head = self.operand(head, false, LEVEL_APP)?;
                let tail = self.operand(tail, false, LEVEL_APP)?;
                let op = if *spread_head { "++" } else { "::" };
                Ok((format!("{head} {op} {tail}"), LEVEL_CONS))
            }
            ExprKind::FunctionAbs { param, domain, body } => {
                let ty = domain_type(domain)?;
                self.scope.push(param.clone());
                let body = self.expr(body, false);
                self.scope.pop();
                Ok((format!("fun ({} : {ty}) => {}", ident(param), body?.0), LEVEL_OPEN))
            }
            other => Err(format!("{} has no Lean translation", construct(other))),
        }
    }

    /// `expr` as an operand: parenthesized when it binds looser than
    /// `max`.
    fn operand(&mut self, expr: &Expr, bare: bool, max: u8) -> Result<String, String> {
        let (text, level) = self.expr(expr, bare)?;
        Ok(if level > max { format!("({text})") } else { text })
    }

    fn path(&mut self, segments: &[String]) -> Result<(String, u8), String> {
        match segments {
            [name] if self.scope.contains(name) => Ok((ident(name), LEVEL_ATOM)),
            [name] => Ok((self.names.resolve(name)?, LEVEL_ATOM)),
            [value, field] if self.scope.contains(value) => match field.as_str() {
                "numer" | "denom" | "length" => Ok((format!("Emath.{field} {}", ident(value)), LEVEL_APP)),
                _ => Err(format!("field `.{field}` has no Lean translation")),
            },
            _ => Err(format!("`{}` has no Lean translation", segments.join("."))),
        }
    }

    fn call(&mut self, function: &Expr, args: &[Expr]) -> Result<(String, u8), String> {
        let (callee, generic) = match &function.kind {
            ExprKind::Path { segments, generics: None } if segments.len() == 1 => {
                let name = &segments[0];
                if self.scope.contains(name) {
                    (ident(name), false)
                } else {
                    (self.names.resolve(name)?, GENERIC_PRELUDE.contains(&name.as_str()))
                }
            }
            _ => (self.operand(function, false, LEVEL_APP)?, false),
        };
        let mut text = callee;
        for arg in args {
            text.push(' ');
            text.push_str(&self.operand(arg, !generic && is_literal(arg), LEVEL_ATOM)?);
        }
        Ok((text, LEVEL_APP))
    }

    fn binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, bare: bool) -> Result<(String, u8), String> {
        // A literal operand beside a typed one takes that type; inside
        // a literal the caller already typed, it stays bare.
        let left_bare = is_literal(left) && (bare || !is_literal(right));
        let right_bare = is_literal(right) && (bare || !is_literal(left));
        let arithmetic = |ctx: &mut Self, symbol: &str, level: u8| -> Result<(String, u8), String> {
            let l = ctx.operand(left, left_bare, level)?;
            let r = ctx.operand(right, right_bare, level - 1)?;
            Ok((format!("{l} {symbol} {r}"), level))
        };
        let relation = |ctx: &mut Self, symbol: &str| -> Result<(String, u8), String> {
            let (l, _) = ctx.expr(left, left_bare)?;
            let (r, _) = ctx.expr(right, right_bare)?;
            Ok((format!("decide ({l} {symbol} {r})"), LEVEL_APP))
        };
        match op {
            BinaryOp::Add => arithmetic(self, "+", LEVEL_ADD),
            BinaryOp::Sub => arithmetic(self, "-", LEVEL_ADD),
            BinaryOp::Mul => arithmetic(self, "*", LEVEL_MUL),
            BinaryOp::Div => {
                // emath `/` is exact rational division, whatever the
                // operands' types.
                let l = if is_literal(left) && !has_division(left) {
                    format!("({} : Rat)", self.expr(left, true)?.0)
                } else {
                    format!("({} : Rat)", self.expr(left, false)?.0)
                };
                let r = self.operand(right, is_literal(right), LEVEL_ATOM.max(LEVEL_MUL - 1))?;
                Ok((format!("{l} / {r}"), LEVEL_MUL))
            }
            BinaryOp::Eq => relation(self, "="),
            BinaryOp::Ne => relation(self, "≠"),
            BinaryOp::Lt => relation(self, "<"),
            BinaryOp::Le => relation(self, "≤"),
            BinaryOp::Gt => relation(self, ">"),
            BinaryOp::Ge => relation(self, "≥"),
            BinaryOp::And => {
                let l = self.operand(left, false, LEVEL_AND)?;
                let r = self.operand(right, false, LEVEL_AND - 1)?;
                Ok((format!("{l} && {r}"), LEVEL_AND))
            }
            BinaryOp::Or => {
                let l = self.operand(left, false, LEVEL_OR)?;
                let r = self.operand(right, false, LEVEL_OR - 1)?;
                Ok((format!("{l} || {r}"), LEVEL_OR))
            }
            other => Err(format!("operator `{other:?}` has no Lean translation")),
        }
    }
}

/// An integer or rational literal, possibly under arithmetic.
fn is_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int(_) | ExprKind::Rational { .. } => true,
        ExprKind::Unary { op: UnaryOp::Neg | UnaryOp::Pos, value } => is_literal(value),
        ExprKind::Binary { op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div, left, right } => {
            is_literal(left) && is_literal(right)
        }
        _ => false,
    }
}

fn has_division(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Rational { .. } | ExprKind::Binary { op: BinaryOp::Div, .. } => true,
        ExprKind::Unary { value, .. } => has_division(value),
        ExprKind::Binary { left, right, .. } => has_division(left) || has_division(right),
        _ => false,
    }
}

fn construct(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Quote { .. } | ExprKind::QuoteBind { .. } => "`quote`",
        ExprKind::Recur { .. } => "`recur`",
        ExprKind::Float(_) => "a decimal literal",
        ExprKind::Str(_) => "text",
        ExprKind::Record { .. } => "a record",
        ExprKind::Tuple(_) => "a tuple",
        ExprKind::Cases { .. } => "`cases`",
        ExprKind::Set(_) | ExprKind::SetComprehension { .. } => "a set",
        ExprKind::Range { .. } => "a `..` range",
        ExprKind::Quantity { .. } | ExprKind::Measured { .. } => "a quantity with units",
        ExprKind::Path { .. } => "a generic path",
        _ => "this expression",
    }
}

fn lean_type(ty: &TypeExpr) -> Result<String, String> {
    match &ty.kind {
        TypeKind::Path { segments, generic_args } => {
            let name = segments.last().map(String::as_str).unwrap_or("");
            match (name, generic_args.as_slice()) {
                ("Int" | "Nat", []) => Ok("Int".into()),
                ("Rat", []) => Ok("Rat".into()),
                ("Bool", []) => Ok("Bool".into()),
                ("sequence", [GenericArg::Type(element)]) => Ok(format!("List {}", atom_type(lean_type(element)?))),
                _ => Err(format!("type `{}` has no Lean translation", segments.join("."))),
            }
        }
        TypeKind::Fn { domain, codomain } => {
            let domain = lean_type(domain)?;
            let domain = if domain.contains('→') { format!("({domain})") } else { domain };
            Ok(format!("{domain} → {}", lean_type(codomain)?))
        }
        _ => Err("this type has no Lean translation".into()),
    }
}

/// The type a closure's `in` domain names.
fn domain_type(domain: &Expr) -> Result<String, String> {
    match &domain.kind {
        ExprKind::Path { segments, generics } => {
            let name = segments.last().map(String::as_str).unwrap_or("");
            match (name, generics.as_deref()) {
                ("Int" | "Nat", None) => Ok("Int".into()),
                ("Rat", None) => Ok("Rat".into()),
                ("Bool", None) => Ok("Bool".into()),
                ("sequence", Some([GenericArg::Type(element)])) => {
                    Ok(format!("List {}", atom_type(lean_type(element)?)))
                }
                _ => Err(format!("domain `{}` has no Lean translation", segments.join("."))),
            }
        }
        ExprKind::Call { function, args } => match (&function.kind, args.as_slice()) {
            (ExprKind::Path { segments, .. }, [element]) if segments.len() == 1 && segments[0] == "sequence" => {
                Ok(format!("List {}", atom_type(domain_type(element)?)))
            }
            _ => Err("this domain has no Lean translation".into()),
        },
        _ => Err("this domain has no Lean translation".into()),
    }
}

fn atom_type(ty: String) -> String {
    if ty.contains(' ') { format!("({ty})") } else { ty }
}

/// A Lean identifier for an emath name: keywords and anything Lean
/// would not read as one plain identifier go inside `«»`.
fn ident(name: &str) -> String {
    let plain = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain && !LEAN_KEYWORDS.contains(&name) {
        name.to_string()
    } else {
        format!("«{name}»")
    }
}

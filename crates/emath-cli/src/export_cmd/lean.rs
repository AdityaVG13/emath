//! emath source to a Lean 4 project.
//!
//! The translated subset: functions over `Int`, `Rat`, `Bool`,
//! `sequence(T)` and function types, with `definitions:` as `let`s,
//! closures, `if`, arithmetic, comparisons, lists, indexing, calls of
//! the file's own functions, of sibling user files (`use`), and of the
//! library functions the embedded prelude (`Prelude.lean`) defines.
//! Every authored `example` becomes a theorem the Lean kernel proves by
//! evaluation (`decide +kernel`), so a wrong value is a Lean error.
//! A function calling `certify_equal(_from)` or `certify_all(_from)`
//! becomes law theorems instead (see `law` and `law_all`). Anything
//! else is refused by name, with the declarations that depend on it;
//! the rest of the file still translates.
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
/// The law library; only a project that certifies laws gets it, and
/// with it a Mathlib dependency.
pub(crate) const LAW: &str = include_str!("Law.lean");
/// Mathlib v4.34.1 and its dependencies at fixed revisions, with
/// `@PACKAGES_DIR@` for the shared package directory. Lake takes the
/// manifest's `packagesDir` over the lakefile's, so both must name it,
/// or Lake clones a fresh Mathlib beside the project and compiles it.
const MANIFEST: &str = include_str!("lake-manifest.json");
pub(crate) const TOOLCHAIN: &str = "leanprover/lean4:v4.34.1\n";
/// Letters, digits and `_`, not starting with a digit: a name Lean (and
/// a Lake glob) reads as one identifier, keywords aside.
fn plain(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The Mathlib modules `Law.lean` imports.
pub(crate) fn law_imports() -> Vec<&'static str> {
    LAW.lines().filter_map(|line| line.strip_prefix("import ")).filter(|module| module.starts_with("Mathlib.")).collect()
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Set by `emath export lean --check` around its own `lake exe cache
/// get`, the one Lake call that may run before Mathlib is built.
pub(crate) const FETCH_ENV: &str = "EMATH_LEAN_FETCH";

/// `lakefile.lean`, not TOML: with laws it refuses to load unless the
/// Mathlib modules `Law.lean` imports are built, so a bare `lake build`
/// can neither clone Mathlib nor compile it (hours of every core). Lake
/// reads `lakefile.lean` over a `lakefile.toml` an older export left.
/// The libraries list their modules, so files an earlier export left
/// (an `Emath/Law.lean`, the Lean file of a deleted source) are not
/// built: they may need Mathlib this project does not require. A
/// module name Lean cannot spell, such as `my-model`, makes `Export`
/// take every file under it.
fn lakefile(mathlib: Option<&Path>, sources: &[&str]) -> String {
    let stems: Vec<&str> = sources.iter().map(|rel| rel.strip_suffix(".emath").unwrap_or(rel)).collect();
    let module = |name: &str| format!(".one {}.toName", quoted(name));
    let export_globs = if stems.iter().all(|stem| stem.split('/').all(plain)) {
        stems.iter().map(|stem| module(&format!("Export.{}", stem.replace('/', ".")))).collect::<Vec<_>>().join(", ")
    } else {
        ".submodules `Export".to_string()
    };
    let mut emath_globs = vec![module("Emath.Prelude"), module("Emath.Checks")];
    let mut text = "import Lake\nopen Lake DSL System\n\n".to_string();
    let mut package = "package emath_export".to_string();
    let mut require = String::new();
    if let Some(packages) = mathlib {
        let dir = quoted(&packages.to_string_lossy());
        let built = quoted(&packages.join("mathlib/.lake/build/lib/lean").to_string_lossy());
        let oleans = law_imports()
            .iter()
            .map(|module| quoted(&format!("{}.olean", module.replace('.', "/"))))
            .collect::<Vec<_>>()
            .join(", ");
        text.push_str(&format!(
            "#eval show IO Unit from do\n  \
             if (← IO.getEnv {fetch}).isNone then\n    \
             for olean in [{oleans}] do\n      \
             unless ← (FilePath.mk {built} / olean).pathExists do\n        \
             throw <| IO.userError (\"Mathlib is not prebuilt in \" ++ {dir} ++\n          \
             \"; `emath export lean --check` fetches its cache. Compiling Mathlib from source takes hours.\")\n\n",
            fetch = quoted(FETCH_ENV)
        ));
        package.push_str(&format!(" where\n  packagesDir := {dir}"));
        require = "\nrequire mathlib from git \"https://github.com/leanprover-community/mathlib4\" @ \"v4.34.1\"\n".to_string();
        emath_globs.push(module("Emath.Law"));
    }
    format!(
        "{text}{package}\n{require}\n@[default_target]\nlean_lib Emath where\n  globs := #[{}]\n\n\
         @[default_target]\nlean_lib Export where\n  globs := #[{export_globs}]\n",
        emath_globs.join(", ")
    )
}

/// The Lean type of a translated value, so that where an `Int` meets a
/// `Rat` (the branches of an `if`, list items, a call argument) the
/// `Int` side is cast, as emath widens it.
#[derive(Clone, Debug, PartialEq)]
enum Ty {
    Int,
    Rat,
    Bool,
    List(Box<Ty>),
    Fn(Box<Ty>, Box<Ty>),
    /// The outputs of a function with several, as the Lean structure of
    /// that (qualified) name: its fields and their types.
    Record(String, Vec<(String, Ty)>),
    /// A type parameter of a generic function's signature, bound at
    /// each call.
    Var(u8),
    /// A type parameter inside the generic function declaring it.
    Param(String),
    /// An integer literal left bare: a neighbour fixes its type.
    Lit,
    Unknown,
}

fn list(element: Ty) -> Ty {
    Ty::List(Box::new(element))
}

fn arrows(params: Vec<Ty>, output: Ty) -> Ty {
    params.into_iter().rev().fold(output, |acc, param| Ty::Fn(Box::new(param), Box::new(acc)))
}

/// Signatures of the library functions and builtins `Prelude.lean`
/// defines, by emath name.
fn prelude_signature(name: &str) -> Option<Ty> {
    let (a, b) = (Ty::Var(0), Ty::Var(1));
    Some(match name {
        "range" => arrows(vec![Ty::Int, Ty::Int], list(Ty::Int)),
        "tabulate" => arrows(vec![arrows(vec![Ty::Int], a.clone()), Ty::Int], list(a)),
        "fold" => arrows(
            vec![arrows(vec![b.clone(), a.clone()], b.clone()), b.clone(), list(a)],
            b,
        ),
        "sum" | "product" | "int_sum" | "int_prod" => arrows(vec![list(Ty::Int)], Ty::Int),
        "length" => arrows(vec![list(a)], Ty::Int),
        "int_quot" | "int_rem" | "int_pow" | "pow_int" => arrows(vec![Ty::Int, Ty::Int], Ty::Int),
        "rat_pow" => arrows(vec![Ty::Rat, Ty::Int], Ty::Rat),
        _ => return None,
    })
}

/// The type both of two values can take: `Int` widens to `Rat`.
fn join(a: &Ty, b: &Ty) -> Ty {
    match (a, b) {
        (Ty::Lit | Ty::Unknown, other) | (other, Ty::Lit | Ty::Unknown) => other.clone(),
        (Ty::Int, Ty::Rat) | (Ty::Rat, Ty::Int) => Ty::Rat,
        (Ty::List(x), Ty::List(y)) => list(join(x, y)),
        _ => a.clone(),
    }
}

/// Whether a `from` value can only become a `to` by dropping a
/// fraction. emath checks a declared type only at a function's
/// outputs, so a `Rat` it passes as an `Int` argument stays a `Rat`;
/// Lean has no such value.
fn narrows(from: &Ty, to: &Ty) -> bool {
    match (from, to) {
        (Ty::Rat, Ty::Int) => true,
        (Ty::List(from), Ty::List(to)) => narrows(from, to),
        (Ty::Fn(from_in, from_out), Ty::Fn(to_in, to_out)) => narrows(to_in, from_in) || narrows(from_out, to_out),
        (Ty::Record(_, from_fields), Ty::Record(_, to_fields)) => from_fields.iter().any(|(name, from)| {
            to_fields.iter().any(|(other, to)| other == name && narrows(from, to))
        }),
        _ => false,
    }
}

/// `coerce`, refusing a value that would narrow.
fn fit(text: String, level: u8, from: &Ty, to: &Ty) -> Result<(String, u8), String> {
    if narrows(from, to) {
        return Err(format!(
            "a `{}` value stands where `{}` is declared, which Lean cannot hold",
            render(from),
            render(to)
        ));
    }
    Ok(coerce(text, level, from, to))
}

/// `text` (of type `from`) as a value of type `to`.
fn coerce(text: String, level: u8, from: &Ty, to: &Ty) -> (String, u8) {
    match (from, to) {
        // Ascribed `Rat` alone, a call to a generic function elaborates
        // against `Rat` and its `Int` arguments no longer fit.
        (Ty::Int, Ty::Rat) if level > LEVEL_ATOM => {
            (format!("(({} : Int) : Rat)", paren(text, level, LEVEL_CONS)), LEVEL_ATOM)
        }
        (Ty::Int, Ty::Rat) => (format!("({text} : Rat)"), LEVEL_ATOM),
        (Ty::List(x), Ty::List(y)) if **x == Ty::Int && **y == Ty::Rat => (
            format!("List.map (fun (x : Int) => (x : Rat)) {}", paren(text, level, LEVEL_ATOM)),
            LEVEL_APP,
        ),
        // emath records with the same fields are one type; Lean's
        // structures of two functions are not.
        (Ty::Record(from_name, from_fields), Ty::Record(to_name, to_fields))
            if from_name != to_name
                && from_fields.len() == to_fields.len()
                && from_fields.iter().all(|(name, _)| to_fields.iter().any(|(other, _)| other == name)) =>
        {
            let fields: Vec<String> = to_fields
                .iter()
                .map(|(name, to)| {
                    let from = &from_fields.iter().find(|(other, _)| other == name).expect("same fields").1;
                    format!("{} := {}", ident(name), coerce(format!("r.{}", ident(name)), LEVEL_ATOM, from, to).0)
                })
                .collect();
            (
                format!("(fun (r : {from_name}) => ({{ {} }} : {to_name})) {}", fields.join(", "), paren(text, level, LEVEL_ATOM)),
                LEVEL_APP,
            )
        }
        _ => (text, level),
    }
}

fn paren(text: String, level: u8, max: u8) -> String {
    if level > max { format!("({text})") } else { text }
}

/// Binds the type parameters in `param` to the matching parts of `arg`.
fn unify(param: &Ty, arg: &Ty, bound: &mut BTreeMap<u8, Ty>) {
    match (param, arg) {
        (Ty::Var(var), _) if !bound.contains_key(var) => {
            let arg = if *arg == Ty::Lit { Ty::Int } else { arg.clone() };
            if arg != Ty::Unknown {
                bound.insert(*var, arg);
            }
        }
        (Ty::List(param), Ty::List(arg)) => unify(param, arg, bound),
        (Ty::Fn(p_in, p_out), Ty::Fn(a_in, a_out)) => {
            unify(p_in, a_in, bound);
            unify(p_out, a_out, bound);
        }
        _ => {}
    }
}

fn substitute(ty: &Ty, bound: &BTreeMap<u8, Ty>) -> Ty {
    match ty {
        Ty::Var(var) => bound.get(var).cloned().unwrap_or(Ty::Var(*var)),
        Ty::List(element) => list(substitute(element, bound)),
        Ty::Fn(domain, codomain) => Ty::Fn(Box::new(substitute(domain, bound)), Box::new(substitute(codomain, bound))),
        other => other.clone(),
    }
}

/// Lean text for a concrete type.
fn render(ty: &Ty) -> String {
    match ty {
        Ty::Int => "Int".into(),
        Ty::Rat => "Rat".into(),
        Ty::Bool => "Bool".into(),
        Ty::List(element) => format!("List {}", atom_type(render(element))),
        Ty::Fn(domain, codomain) => {
            let domain = render(domain);
            let domain = if domain.contains('→') { format!("({domain})") } else { domain };
            format!("{domain} → {}", render(codomain))
        }
        Ty::Record(name, _) => name.clone(),
        Ty::Param(name) => ident(name),
        Ty::Var(_) | Ty::Lit | Ty::Unknown => "_".into(),
    }
}

/// `ty` with the type parameters `generics` names as call-site
/// variables.
fn generalize(ty: &Ty, generics: &[String]) -> Ty {
    match ty {
        Ty::Param(name) => match generics.iter().position(|generic| generic == name) {
            Some(index) => Ty::Var(u8::try_from(index).unwrap_or(u8::MAX)),
            None => ty.clone(),
        },
        Ty::List(element) => list(generalize(element, generics)),
        Ty::Fn(domain, codomain) => {
            Ty::Fn(Box::new(generalize(domain, generics)), Box::new(generalize(codomain, generics)))
        }
        other => other.clone(),
    }
}

fn has_param(ty: &Ty) -> bool {
    match ty {
        Ty::Param(_) => true,
        Ty::List(element) => has_param(element),
        Ty::Fn(domain, codomain) => has_param(domain) || has_param(codomain),
        Ty::Record(_, fields) => fields.iter().any(|(_, ty)| has_param(ty)),
        _ => false,
    }
}

/// Every word token of Lean with the Mathlib modules `Law.lean`
/// imports (`Parser.getTokenTable`, Lean 4.34.1), plus `obtain` and
/// `rec`: a field or binder spelled as one of these ends the
/// declaration it appears in.
const LEAN_KEYWORDS: &[&str] = &[
    "abbrev", "add_aesop_rules", "add_decl_doc", "alias", "assert_not_exists", "assert_not_imported",
    "assumeInstancesCommuteDummy", "at", "attribute", "axiom", "bif", "binder_predicate", "break",
    "builtin_cbv_simproc", "builtin_cbv_simproc_decl", "builtin_dsimproc", "builtin_dsimproc_decl",
    "builtin_grind_propagator", "builtin_initialize", "builtin_simproc", "builtin_simproc_decl", "by",
    "by_elab", "calc", "catch", "cbv_eval", "cbv_simproc", "cbv_simproc_decl", "class", "coinductive",
    "coinductive_fixpoint", "continue", "dbg_trace", "declare_aesop_rule_sets",
    "declare_bitwise_int_theorems", "declare_bitwise_uint_theorems", "declare_command_config_elab",
    "declare_command_config_elab_legacy", "declare_config_elab", "declare_config_elab_legacy",
    "declare_core_config_elab", "declare_int_theorems", "declare_simp_like_tactic",
    "declare_sint_simprocs", "declare_syntax_cat", "declare_term_config_elab", "declare_uint_simprocs",
    "declare_uint_theorems", "decreasing_by", "def", "def_eval_config_item", "def_wanted", "deprecate",
    "deprecated_module", "deprecated_syntax", "deriving", "do", "docs_to_verso", "dsimproc",
    "dsimproc_decl", "elab", "elab_rules", "elab_stx_quot", "else", "end", "erase_aesop_rules",
    "eval_prec", "eval_prio", "example", "exists", "export", "extend_docs", "extends", "finally",
    "for", "forall", "from", "fun", "generalizing", "GL", "grind_annotated", "grind_pattern",
    "grind_propagator", "guard_min_heartbeats", "have", "haveI", "hiding", "idbg", "if", "import",
    "in", "include", "include_str", "inductive", "inductive_fixpoint", "inferInstanceAs", "infix",
    "infixl", "infixr", "init_grind_norm", "init_quot", "initialize", "initialize_simps_projections",
    "insert_to_additive_translation", "insert_to_dual_translation", "instance", "instance_wanted",
    "irreducible_def", "kerodon", "leading_parser", "lemma", "let", "let_delayed", "let_expr",
    "let_fun", "let_impl_detail", "let_tmp", "letI", "library_note", "local", "logNamedError",
    "logNamedErrorAt", "logNamedWarning", "logNamedWarningAt", "macro", "macro_rules", "match",
    "match_expr", "matches", "max_prec", "meta", "mk_iff_of_inductive_prop", "mod_cast", "mut",
    "mutual", "namespace", "nat_lit", "no_index", "nofun", "nomatch", "noncomputable", "nonrec",
    "norm_cast_add_elim", "notation", "notation3", "obtain", "omit", "opaque", "open", "partial",
    "partial_fixpoint", "PiType", "postfix", "prefix", "private", "proof_wanted", "Prop", "protected",
    "public", "rec", "recommended_spelling", "register_builtin_option", "register_error_explanation",
    "register_grind_attr", "register_hint", "register_label_attr", "register_linter_set",
    "register_option", "register_parser_alias", "register_simp_attr", "register_sym_dsimp",
    "register_sym_simp", "register_tactic_tag", "renaming", "repeat", "return", "run_cmd", "run_elab",
    "run_meta", "says", "scoped", "seal", "section", "set_library_suggestions", "set_option", "show",
    "show_panel_widgets", "show_term", "show_term_elab", "simproc", "simproc_decl", "sorry", "Sort",
    "stacks", "StateRefT", "structure", "sudo", "suffices", "syntax", "tactic_alt", "tactic_extension",
    "tactic_name", "tactic_tag", "termination_by", "then", "theorem", "theorem_wanted", "throwError",
    "throwErrorAt", "throwNamedError", "throwNamedErrorAt", "to", "to_additive_name_hint",
    "to_dual_insert_cast", "to_dual_insert_cast_fun", "to_dual_name_hint", "trailing_parser", "try",
    "Type", "unif_hint", "universe", "unless", "unlock_limits", "unsafe", "unseal", "unset_option",
    "until", "using", "variable", "variables", "whatsnew", "where", "while", "with",
    "with_annotate_term", "with_weak_namespace", "without_expected_type",
];

/// One authored example as a theorem, for naming Lean's verdict.
pub(crate) struct ExampleSite {
    pub lean: String,
    pub first_line: usize,
    pub last_line: usize,
    pub what: String,
}

/// One certified law as a theorem: the lines of its two order bounds,
/// of its two bounds in the parameters (`certify_all`) and of its check.
pub(crate) struct LawSite {
    pub lean: String,
    pub first_line: usize,
    pub order_lines: [usize; 2],
    pub bound_lines: Vec<usize>,
    pub check_line: usize,
    pub last_line: usize,
    pub what: String,
    /// The `.emath` file stating the law, and the law's call as emath
    /// source, for asking emath whether it certifies the law too.
    pub source: PathBuf,
    pub emath_call: String,
}

/// A law theorem and the offsets of its lines from its first line.
struct LawText {
    text: String,
    order: [usize; 2],
    bound: Vec<usize>,
    check: usize,
}

/// A translated user function, for reading the divisors and branch
/// conditions a law reaches: its declaration and the qualified Lean
/// names of the user functions its file can call.
struct Body {
    decl: Declaration,
    names: BTreeMap<String, String>,
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
    pub laws: usize,
    pub sites: Vec<ExampleSite>,
    pub law_sites: Vec<LawSite>,
    /// Each translated function's qualified Lean name and the user
    /// functions it calls.
    calls: BTreeMap<String, BTreeSet<String>>,
    /// Each translated function by qualified Lean name.
    bodies: BTreeMap<String, Body>,
}

struct Source {
    path: PathBuf,
    text: String,
    rel: String,
    tree: SyntaxTree,
    /// (selection, file) for each sibling user file this one uses.
    local_uses: Vec<(UseTree, PathBuf)>,
    /// (dotted path, selection, file) for each library module.
    library_uses: Vec<(String, UseTree, PathBuf)>,
    /// A library module outside the exported folder: its functions
    /// translate for the files that use it, its examples and laws do not.
    library: bool,
    /// Why emath refuses a library module: nothing in it translates.
    refused: Option<String>,
}

/// What a file's translation offers to files that `use` it.
struct Module {
    namespace: String,
    module: String,
    exports: Option<Vec<String>>,
    /// Translated functions and their signatures.
    translated: BTreeMap<String, Ty>,
    functions: BTreeSet<String>,
    /// Translated objects (its own and those it sees), by emath name.
    objects: BTreeMap<String, Ty>,
    library: bool,
    /// The `use` path of a library module.
    display: String,
    /// Why each function that did not translate did not.
    reasons: BTreeMap<String, String>,
    /// The functions it imports, for an `exports:` that passes them on.
    imported: BTreeMap<String, Target>,
}

#[derive(Clone)]
enum Target {
    Lean(String, Ty),
    /// A function of the recursive group being translated, by Lean
    /// name: calls go to its fuel form, spending one fuel.
    Fuel(String, Ty),
    /// Not translated; a library function carries why, as no refusal
    /// names it otherwise.
    Refused(Option<String>),
}

/// Name resolution for one file: its own functions, its sibling
/// imports, the library modules it uses, then the prelude.
#[derive(Default)]
struct Names {
    namespace: String,
    targets: BTreeMap<String, Target>,
    library: BTreeMap<String, String>,
    used_prelude: BTreeSet<String>,
    /// Qualified names of the user functions resolved since last cleared.
    called: BTreeSet<String>,
    /// The translated `emath object`s in scope, by emath name.
    objects: BTreeMap<String, Ty>,
}

/// emath's machine integer operations. A call by one of these names
/// runs the machine even where a declaration of that name is in scope
/// (`exact.modular` declares a guarded `int_rem` wrapping it), so the
/// translation resolves them to the prelude first, as emath does.
fn machine_int(name: &str) -> bool {
    matches!(name, "int_quot" | "int_rem" | "int_pow" | "int_sum" | "int_prod")
}

impl Names {
    fn resolve(&mut self, name: &str) -> Result<(String, Ty), String> {
        if machine_int(name)
            && let Some(ty) = prelude_signature(name)
        {
            self.used_prelude.insert(name.to_string());
            return Ok((format!("Emath.{name}"), ty));
        }
        match self.targets.get(name) {
            Some(Target::Lean(lean, ty)) => {
                let qualified =
                    if lean.contains('.') { lean.clone() } else { format!("{}.{lean}", self.namespace) };
                self.called.insert(qualified);
                return Ok((lean.clone(), ty.clone()));
            }
            Some(Target::Fuel(lean, ty)) => {
                self.called.insert(format!("{}.{lean}", self.namespace));
                return Ok((format!("{lean}.fuel {FUEL}"), ty.clone()));
            }
            Some(Target::Refused(Some(reason))) => return Err(reason.clone()),
            Some(Target::Refused(None)) => return Err(format!("calls `{name}`, which is not translated")),
            None => {}
        }
        if let Some(ty) = prelude_signature(name) {
            self.used_prelude.insert(name.to_string());
            return Ok((format!("Emath.{name}"), ty));
        }
        match self.library.get(name) {
            Some(module) => Err(format!("`{name}` (from `{module}`) has no Lean translation")),
            None => Err(format!("`{name}` has no Lean translation")),
        }
    }
}

/// Translate `roots` (absolute `.emath` paths under `base`) and every
/// sibling user file they reach. A project with laws depends on the
/// Mathlib packages in `mathlib`.
pub(crate) fn export(roots: &[PathBuf], base: &Path, mathlib: &Path) -> Result<Export, String> {
    let mut sources: BTreeMap<PathBuf, Source> = BTreeMap::new();
    let mut order = Vec::new();
    let mut visiting = BTreeSet::new();
    for root in roots {
        load(root, None, base, &mut sources, &mut order, &mut visiting)?;
    }
    let mut libraries: BTreeMap<PathBuf, SyntaxTree> = BTreeMap::new();
    let mut modules: BTreeMap<PathBuf, Module> = BTreeMap::new();
    let mut export = Export {
        sources: order.iter().filter(|path| !sources[*path].library).count(),
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
    let rels: Vec<&str> = order.iter().map(|path| sources[path].rel.as_str()).collect();
    if export.laws > 0 {
        let manifest = MANIFEST.replace("\"@PACKAGES_DIR@\"", &quoted(&mathlib.to_string_lossy()));
        export.files.insert(0, ("Emath/Law.lean".to_string(), LAW.to_string()));
        export.files.insert(0, ("lake-manifest.json".to_string(), manifest));
        export.files.insert(0, ("lakefile.lean".to_string(), lakefile(Some(mathlib), &rels)));
    } else {
        // Replaces the Mathlib manifest an earlier export with laws wrote.
        let manifest = "{\"version\": \"1.2.0\",\n \"packagesDir\": \".lake/packages\",\n \"packages\": [],\n \
                        \"name\": \"emath_export\",\n \"lakeDir\": \".lake\",\n \"fixedToolchain\": false}\n";
        export.files.insert(0, ("lake-manifest.json".to_string(), manifest.to_string()));
        export.files.insert(0, ("lakefile.lean".to_string(), lakefile(None, &rels)));
    }
    Ok(export)
}

/// Loads `path` and what it uses, callees first into `order`. `library`:
/// the dotted path of a library module, which translates under `lib/`
/// unless it lies in the exported folder.
fn load(
    path: &Path,
    library: Option<&str>,
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
    let library = library.filter(|_| relative(path, base).is_none());
    let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let (tree, diagnostics) = emath_syntax::parse_str(&text);
    if diagnostics.has_errors() {
        return Err(format!("{}: does not parse; run `emath check` on it", path.display()));
    }
    // Lean must never accept a file emath refuses. emath admits a file
    // that uses a library module it would refuse on its own; that
    // module's functions then do not translate.
    let (admission, _, _) = crate::run_check(path);
    let refused = admission.errors().next().map(|error| format!("does not admit: {}: {}", error.code, error.message));
    if let Some(refused) = &refused
        && library.is_none()
    {
        return Err(format!("{}: {refused}; run `emath check` on it", path.display()));
    }
    let rel = match library {
        Some(dotted) => format!("lib/{}.emath", dotted.replace('.', "/")),
        None => relative(path, base)
            .ok_or_else(|| format!("{} is outside the exported folder {}", path.display(), base.display()))?,
    };
    let mut local_uses = Vec::new();
    let mut library_uses = Vec::new();
    for item in tree.items.iter().filter(|_| refused.is_none()) {
        let Item::Use { path: use_path, tree: selection, .. } = item else {
            continue;
        };
        let (file, is_library) = emath_exec_ir::constructor_layer::resolve_use(&tree, use_path, path)
            .map_err(|error| format!("{rel}: {}: {}", error.code, error.message))?;
        let file = file.canonicalize().unwrap_or(file);
        let dotted = use_path.join(".");
        load(&file, is_library.then_some(dotted.as_str()), base, sources, order, visiting)?;
        if is_library {
            library_uses.push((dotted, selection.clone(), file.clone()));
        }
        local_uses.push((selection.clone(), file));
    }
    visiting.remove(path);
    order.push(path.to_path_buf());
    sources.insert(
        path.to_path_buf(),
        Source { path: path.to_path_buf(), text, rel, tree, local_uses, library_uses, library: library.is_some(), refused },
    );
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
    let mut names = Names { namespace: module.clone(), ..Names::default() };
    let mut imports = vec!["Emath.Prelude".to_string()];
    for (selection, file) in &source.local_uses {
        let imported = &modules[file];
        let available = imported
            .exports
            .clone()
            .unwrap_or_else(|| imported.functions.iter().cloned().collect());
        // What a used file imports is visible here too, as in emath; the
        // file's own selection takes precedence.
        for (name, target) in &imported.imported {
            names.targets.entry(name.clone()).or_insert_with(|| target.clone());
        }
        // Imported names stay qualified: an imported file's private
        // functions may share a name with this file's.
        for (local, name) in selected(selection, available.into_iter()) {
            // The prelude's own Lean definitions of these, checked
            // against the library's examples, stay the ones called.
            if imported.library && prelude_signature(&name).is_some() {
                continue;
            }
            let target = match imported.translated.get(&name) {
                Some(ty) => Target::Lean(format!("{}.{}", imported.namespace, ident(&name)), ty.clone()),
                None if let Some(target) = imported.imported.get(&name) => target.clone(),
                // Its refusal is not reported on its own.
                None if imported.library => Target::Refused(imported.reasons.get(&name).map(|reason| {
                    format!("calls `{name}` from `{}`, which does not translate: {}", imported.display, root_cause(reason))
                })),
                None => Target::Refused(None),
            };
            names.targets.insert(local, target);
        }
        names.objects.extend(imported.objects.iter().map(|(name, ty)| (name.clone(), ty.clone())));
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
    let own_names: BTreeSet<String> = functions(&source.tree).map(|decl| decl.name.clone()).collect();
    let (certifiers, own): (Vec<&Declaration>, Vec<&Declaration>) =
        functions(&source.tree).partition(|decl| !certify_calls(decl, &names).is_empty());
    for item in &source.tree.items {
        if let Item::Declaration(decl) = item
            && !matches!(decl.as_kind.as_str(), "function" | "object")
        {
            refusals.push(format!(
                "not translated: {} `{}`: an emath {} has no Lean translation yet",
                source.rel, decl.name, decl.as_kind
            ));
        }
    }
    // An object's fields may name objects declared after it.
    let mut pending: Vec<&Declaration> = source
        .tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration(decl) if decl.as_kind == "object" && source.refused.is_none() => Some(decl),
            _ => None,
        })
        .collect();
    loop {
        let before = pending.len();
        let mut failed = Vec::new();
        for decl in pending {
            match object(decl, &names) {
                Ok((text, ty)) => {
                    body.push_str(&text);
                    names.objects.insert(decl.name.clone(), ty);
                }
                Err(reason) => failed.push((decl, reason)),
            }
        }
        if failed.len() == before {
            for (decl, reason) in failed {
                refusals.push(format!("not translated: {} `{}`: {reason}", source.rel, decl.name));
            }
            break;
        }
        pending = failed.into_iter().map(|(decl, _)| decl).collect();
    }
    let mut translated = BTreeMap::new();
    let mut reasons = BTreeMap::new();
    let plain_names: BTreeSet<String> = own.iter().map(|decl| decl.name.clone()).collect();
    for (group, recursive) in ordered(&own, &plain_names) {
        names.called.clear();
        let result = if let Some(refused) = &source.refused {
            Err(format!("its module {refused}"))
        } else if recursive {
            recursive_group(&group, &mut names)
        } else {
            function(group[0], &mut names).map(|(text, signature)| (text, vec![signature]))
        };
        match result {
            Ok((text, signatures)) => {
                body.push_str(&text);
                let called = std::mem::take(&mut names.called);
                for (decl, signature) in group.iter().zip(signatures) {
                    export.calls.insert(format!("{module}.{}", ident(&decl.name)), called.clone());
                    names.targets.insert(decl.name.clone(), Target::Lean(ident(&decl.name), signature.clone()));
                    translated.insert(decl.name.clone(), signature);
                    export.functions += usize::from(!source.library);
                }
            }
            Err(reason) => {
                for decl in &group {
                    refusals.push(format!("not translated: {} `{}`: {reason}", source.rel, decl.name));
                    reasons.insert(decl.name.clone(), reason.clone());
                    // A library's refusals are not reported on their own.
                    let target = source.library.then(|| {
                        format!("calls `{}`, which does not translate: {}", decl.name, root_cause(&reason))
                    });
                    names.targets.insert(decl.name.clone(), Target::Refused(target));
                }
            }
        }
    }
    for name in own_names.iter().filter(|name| !translated.contains_key(*name)) {
        names.targets.insert(name.clone(), Target::Refused(None));
    }
    let visible = visible(&names);
    for decl in &own {
        if translated.contains_key(&decl.name) {
            export.bodies.insert(
                format!("{module}.{}", ident(&decl.name)),
                Body { decl: (*decl).clone(), names: visible.clone() },
            );
        }
    }
    let mut laws = Vec::new();
    for decl in certifiers.iter().filter(|_| !source.library) {
        for case in examples(decl) {
            refusals.push(format!(
                "not translated: {} example <{}> of `{}`: a function that certifies laws becomes the laws, not a Lean value",
                source.rel, case.label, decl.name
            ));
        }
        for (k, (call, args)) in certify_calls(decl, &names).into_iter().enumerate() {
            let what = format!("{} law {} of `{}`", source.rel, k + 1, decl.name);
            let name = format!("{}.law_{}", ident(&decl.name), k + 1);
            match law(&name, call, args, &mut names, &export.calls, &export.bodies) {
                Ok(text) => laws.push((text, what, emath_call(&source.text, call, args))),
                Err(reason) => refusals.push(format!("not translated: {what}: {reason}")),
            }
        }
    }
    if !laws.is_empty() {
        imports.push("Emath.Law".to_string());
    }

    let mut text = format!(
        "/-\nGenerated by `emath export lean` from {}; the next export\noverwrites it.\n-/\n",
        source.rel
    );
    for import in &imports {
        text.push_str(&format!("import {import}\n"));
    }
    // emath binders may go unused (a fold's index, say).
    text.push_str("\nset_option linter.unusedVariables false\n");
    // The kernel's default depth stops a recursive function's example
    // long before emath's work budget does.
    text.push_str("set_option maxRecDepth 1000000\n\n");
    text.push_str(&format!("namespace {module}\n\n"));
    let lean_path = lean_path(&source.rel);
    text.push_str(&body);
    let with_examples: BTreeSet<String> = own
        .iter()
        .filter(|decl| !source.library && translated.contains_key(&decl.name) && !examples(decl).is_empty())
        .map(|decl| decl.name.clone())
        .collect();
    let faults = if with_examples.is_empty() { BTreeMap::new() } else { emath_faults(source, &with_examples)? };
    for decl in &own {
        if !with_examples.contains(&decl.name) {
            continue;
        }
        let params = section_typed_fields(decl, "inputs");
        let outputs = section_typed_fields(decl, "outputs");
        let mut index = 0;
        for case in examples(decl) {
            let what = format!("{} example <{}> of `{}`", source.rel, case.label, decl.name);
            let key = (decl.name.clone(), (!case.loose).then_some(index));
            index += usize::from(!case.loose);
            if let Some(code) = faults.get(&key) {
                refusals.push(format!("not translated: {what}: it faults in emath (`{code}`), so it has no value to prove"));
                continue;
            }
            match theorem(&decl.name, &ident(&decl.name), &params, &outputs, &case, &mut names) {
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
    for (law, what, emath_call) in laws {
        let first_line = text.lines().count() + 1;
        text.push_str(&law.text);
        export.law_sites.push(LawSite {
            lean: lean_path.clone(),
            first_line,
            order_lines: law.order.map(|offset| first_line + offset),
            bound_lines: law.bound.iter().map(|offset| first_line + offset).collect(),
            check_line: first_line + law.check,
            last_line: text.lines().count(),
            what,
            source: source.path.clone(),
            emath_call,
        });
        export.laws += 1;
    }
    text.push_str(&format!("end {module}\n"));
    let namespace = module.clone();
    let imported = names
        .targets
        .iter()
        .filter(|(name, _)| !own_names.contains(*name))
        .map(|(name, target)| (name.clone(), target.clone()))
        .collect();
    Ok((
        Module {
            namespace,
            module,
            exports: exports(&source.tree),
            translated,
            functions: own_names,
            objects: names.objects.clone(),
            library: source.library,
            display: source.rel.strip_prefix("lib/").and_then(|rel| rel.strip_suffix(".emath")).unwrap_or(&source.rel).replace('/', "."),
            reasons,
            imported,
        },
        text,
        // A library function that does not translate is named where a
        // user function calls it.
        if source.library { Vec::new() } else { refusals },
        names.used_prelude,
    ))
}

/// The file's functions in groups that call each other, callees'
/// groups first (Tarjan's strongly connected components). A group of
/// one is recursive only if it calls itself.
fn ordered<'a>(own: &[&'a Declaration], own_names: &BTreeSet<String>) -> Vec<(Vec<&'a Declaration>, bool)> {
    struct Walk<'a, 'b> {
        by_name: BTreeMap<&'b str, &'a Declaration>,
        callees: BTreeMap<&'b str, Vec<String>>,
        index: BTreeMap<String, usize>,
        low: BTreeMap<String, usize>,
        stack: Vec<&'a Declaration>,
        on_stack: BTreeSet<String>,
        out: Vec<(Vec<&'a Declaration>, bool)>,
    }
    impl<'a> Walk<'a, '_> {
        fn visit(&mut self, decl: &'a Declaration) {
            let at = self.index.len();
            self.index.insert(decl.name.clone(), at);
            self.low.insert(decl.name.clone(), at);
            self.stack.push(decl);
            self.on_stack.insert(decl.name.clone());
            for callee in self.callees[decl.name.as_str()].clone() {
                if !self.index.contains_key(&callee) {
                    self.visit(self.by_name[callee.as_str()]);
                    let low = self.low[&decl.name].min(self.low[&callee]);
                    self.low.insert(decl.name.clone(), low);
                } else if self.on_stack.contains(&callee) {
                    let low = self.low[&decl.name].min(self.index[&callee]);
                    self.low.insert(decl.name.clone(), low);
                }
            }
            if self.low[&decl.name] == at {
                let mut group = Vec::new();
                while let Some(member) = self.stack.pop() {
                    self.on_stack.remove(&member.name);
                    group.push(member);
                    if member.name == decl.name {
                        break;
                    }
                }
                group.reverse();
                let recursive = group.len() > 1 || self.callees[decl.name.as_str()].contains(&decl.name);
                self.out.push((group, recursive));
            }
        }
    }
    let mut walk = Walk {
        by_name: own.iter().map(|decl| (decl.name.as_str(), *decl)).collect(),
        callees: BTreeMap::new(),
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        out: Vec::new(),
    };
    for decl in own {
        let mut callees = BTreeSet::new();
        for stmt in &decl.body {
            collect_stmt_names(stmt, &mut callees);
        }
        let callees = callees.into_iter().filter(|name| own_names.contains(name) && !machine_int(name)).collect();
        walk.callees.insert(decl.name.as_str(), callees);
    }
    for decl in own {
        if !walk.index.contains_key(&decl.name) {
            walk.visit(decl);
        }
    }
    walk.out
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
    visit(expr, &mut |expr| {
        if let ExprKind::Path { segments, .. } = &expr.kind
            && let Some(first) = segments.first()
        {
            names.insert(first.clone());
        }
    });
}

/// The Lean definition and its signature.
/// Runs `f` with `decl`'s type parameters in scope as types.
fn with_generics<R>(
    decl: &Declaration,
    names: &mut Names,
    f: impl FnOnce(&mut Names) -> Result<R, String>,
) -> Result<R, String> {
    if decl.generics.iter().any(|generic| generic.bound.is_some()) {
        return Err("a bounded type parameter has no Lean translation yet".into());
    }
    let saved: Vec<(String, Option<Ty>)> = decl
        .generics
        .iter()
        .map(|generic| {
            let old = names.objects.insert(generic.name.clone(), Ty::Param(generic.name.clone()));
            (generic.name.clone(), old)
        })
        .collect();
    let result = f(names);
    for (name, old) in saved.into_iter().rev() {
        match old {
            Some(ty) => names.objects.insert(name, ty),
            None => names.objects.remove(&name),
        };
    }
    result
}

fn function(decl: &Declaration, names: &mut Names) -> Result<(String, Ty), String> {
    with_generics(decl, names, |names| function_body(decl, names))
}

fn function_body(decl: &Declaration, names: &mut Names) -> Result<(String, Ty), String> {
    let shape = shape(decl, names)?;
    let body = definitions(decl, &shape, names)?;
    let text = format!(
        "{}def {}{} : {} :=\n{}  {}\n\n",
        shape.structure,
        ident(&decl.name),
        shape.binders(),
        render(&shape.output),
        body.lines.iter().map(|line| format!("  {line}\n")).collect::<String>(),
        body.result
    );
    Ok((text, shape.signature()))
}

/// A function's declared Lean types: its parameters, its output (a
/// structure `f.Out` when it has several, as emath returns several
/// outputs as a record with those fields) and that structure's text.
struct Shape {
    generics: Vec<String>,
    params: Vec<(String, Ty)>,
    outputs: Vec<(String, Ty)>,
    output: Ty,
    structure: String,
}

impl Shape {
    /// Type parameters are implicit, each `Inhabited` for `default`
    /// and `Emath.index`; Lean infers them at each call.
    fn binders(&self) -> String {
        let generics: String = self
            .generics
            .iter()
            .map(|name| format!(" {{{} : Type}} [Inhabited {}]", ident(name), ident(name)))
            .collect();
        let params: String =
            self.params.iter().map(|(name, ty)| format!(" ({} : {})", ident(name), render(ty))).collect();
        format!("{generics}{params}")
    }

    fn signature(&self) -> Ty {
        let signature = arrows(self.params.iter().map(|(_, ty)| ty.clone()).collect(), self.output.clone());
        generalize(&signature, &self.generics)
    }
}

/// An `emath object` as a Lean structure of its representation's
/// fields, and its type.
fn object(decl: &Declaration, names: &Names) -> Result<(String, Ty), String> {
    if !decl.generics.is_empty() {
        return Err("generic objects have no Lean translation yet".into());
    }
    for section in decl.sections() {
        if section.name != "representation" {
            return Err(format!("section `{}:` has no Lean translation", section.name));
        }
    }
    let mut fields = Vec::new();
    for (name, ty) in section_typed_fields(decl, "representation") {
        fields.push((name, lean_type(&ty, &names.objects)?));
    }
    if fields.is_empty() {
        return Err("an object without fields has no Lean translation".into());
    }
    let text = structure_text(&ident(&decl.name), &fields);
    Ok((text, Ty::Record(format!("{}.{}", names.namespace, ident(&decl.name)), fields)))
}

fn shape(decl: &Declaration, names: &Names) -> Result<Shape, String> {
    let namespace = &names.namespace;
    for section in decl.sections() {
        if !matches!(section.name.as_str(), "inputs" | "outputs" | "definitions" | "tests") {
            return Err(format!("section `{}:` has no Lean translation", section.name));
        }
    }
    let mut params = Vec::new();
    for (name, ty) in section_typed_fields(decl, "inputs") {
        params.push((name, lean_type(&ty, &names.objects)?));
    }
    let mut outputs = Vec::new();
    for (name, ty) in section_typed_fields(decl, "outputs") {
        outputs.push((name, lean_type(&ty, &names.objects)?));
    }
    let mut structure = String::new();
    let output = match outputs.as_slice() {
        [] => return Err("a function without outputs has no Lean translation".into()),
        [(_, ty)] => ty.clone(),
        _ if !decl.generics.is_empty() => {
            return Err("a generic function with several outputs has no Lean translation yet".into());
        }
        _ => {
            let local = format!("{}.Out", ident(&decl.name));
            structure = structure_text(&local, &outputs);
            Ty::Record(format!("{namespace}.{local}"), outputs.clone())
        }
    };
    let generics = decl.generics.iter().map(|generic| generic.name.clone()).collect();
    Ok(Shape { generics, params, outputs, output, structure })
}

/// A Lean structure with `fields`, with decidable equality where every
/// field has it (no function inside).
fn structure_text(name: &str, fields: &[(String, Ty)]) -> String {
    let mut text = format!("structure {name} where\n");
    for (field, ty) in fields {
        text.push_str(&format!("  {} : {}\n", ident(field), render(ty)));
    }
    let decidable = fields.iter().all(|(_, ty)| decidable(ty));
    text.push_str(if decidable { "deriving DecidableEq, Inhabited\n\n" } else { "deriving Inhabited\n\n" });
    text
}

/// Whether Lean decides equality of `ty`: not of functions, nor of a
/// type parameter (emath compares neither).
fn decidable(ty: &Ty) -> bool {
    match ty {
        Ty::Fn(..) | Ty::Param(_) => false,
        Ty::List(element) => decidable(element),
        Ty::Record(_, fields) => fields.iter().all(|(_, ty)| decidable(ty)),
        _ => true,
    }
}

/// A function's definitions as Lean `let` lines and its result.
struct Definitions {
    lines: Vec<String>,
    result: String,
}

fn definitions(decl: &Declaration, shape: &Shape, names: &mut Names) -> Result<Definitions, String> {
    let mut ctx = Ctx { names, scope: shape.params.clone() };
    let mut lines = Vec::new();
    let mut assigned = BTreeMap::new();
    for section in decl.sections().filter(|section| section.name == "definitions") {
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::Assign { target, value } if target.segments.len() == 1 && target.indices.is_empty() => {
                    let name = &target.segments[0];
                    let (value, _, ty) = ctx.expr(value, false)?;
                    lines.push(format!("let {} := {value}", ident(name)));
                    assigned.insert(name.clone(), ty.clone());
                    ctx.scope.push((name.clone(), ty));
                }
                StmtKind::Require(_) => {}
                _ => return Err("a definition that is not `name = value` has no Lean translation".into()),
            }
        }
    }
    let mut results = Vec::new();
    for (output, ty) in &shape.outputs {
        let Some(result_ty) = assigned.get(output) else {
            return Err(format!("output `{output}` is never defined"));
        };
        results.push((output, fit(ident(output), LEVEL_ATOM, result_ty, ty)?.0));
    }
    let result = match results.as_slice() {
        [(_, result)] => result.clone(),
        _ => {
            let fields: Vec<String> =
                results.iter().map(|(output, result)| format!("{} := {result}", ident(output))).collect();
            format!("{{ {} }}", fields.join(", "))
        }
    };
    Ok(Definitions { lines, result })
}

/// Not an emath name, so no parameter shadows it.
const FUEL: &str = "«emath fuel»";

/// A group of functions that call each other (or one that calls
/// itself). Lean accepts a definition only with a termination proof,
/// so each becomes `f.fuel`, recursion on a fuel count that every call
/// spends one of, and `f` runs it with 2^64 fuel. emath spends at least
/// one unit of its `u64` work count per nested call, so every value emath
/// computes, under any budget, ends before the fuel does and `f` there is
/// emath's value.
fn recursive_group(group: &[&Declaration], names: &mut Names) -> Result<(String, Vec<Ty>), String> {
    let mut shapes = Vec::new();
    for decl in group {
        let shape = with_generics(decl, names, |names| shape(decl, names))?;
        names.targets.insert(decl.name.clone(), Target::Fuel(ident(&decl.name), shape.signature()));
        shapes.push(shape);
    }
    let mut text: String = shapes.iter().map(|shape| shape.structure.as_str()).collect();
    let mut defs = Vec::new();
    for (decl, shape) in group.iter().zip(&shapes) {
        let body = with_generics(decl, names, |names| definitions(decl, shape, names))?;
        defs.push(format!(
            "def {}.fuel ({FUEL} : Nat){} : {} :=\n  match {FUEL} with\n  | 0 => default\n  | {FUEL} + 1 =>\n{}    {}\n",
            ident(&decl.name),
            shape.binders(),
            render(&shape.output),
            body.lines.iter().map(|line| format!("    {line}\n")).collect::<String>(),
            body.result
        ));
    }
    if defs.len() == 1 {
        text.push_str(&defs[0]);
        text.push('\n');
    } else {
        text.push_str(&format!("mutual\n\n{}\nend\n\n", defs.join("\n")));
    }
    let fuel = u128::from(u64::MAX) + 1;
    for (decl, shape) in group.iter().zip(&shapes) {
        let args: String = shape.params.iter().map(|(name, _)| format!(" {}", ident(name))).collect();
        text.push_str(&format!(
            "def {name}{} : {} :=\n  {name}.fuel {fuel}{args}\n\n",
            shape.binders(),
            render(&shape.output),
            name = ident(&decl.name),
        ));
    }
    Ok((text, shapes.iter().map(Shape::signature).collect()))
}

struct Case {
    label: String,
    givens: Vec<(String, Expr)>,
    expects: Vec<Expr>,
    /// Givens and expects written outside any `example`.
    loose: bool,
}

fn examples(decl: &Declaration) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    let mut labels = BTreeMap::<String, usize>::new();
    let mut loose = Case { label: decl.name.clone(), givens: Vec::new(), expects: Vec::new(), loose: true };
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
                        loose: false,
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

/// The examples of the `exported` functions that fault when emath runs
/// them, keyed by function and index among its `example`s (`None`: the
/// loose givens and expects). Lean's arithmetic is total (`x / 0 = 0`),
/// so such an example would get a value in Lean it never has in emath.
fn emath_faults(
    source: &Source,
    exported: &BTreeSet<String>,
) -> Result<BTreeMap<(String, Option<usize>), String>, String> {
    let mut tree = source.tree.clone();
    for item in &mut tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind != "function" || !exported.contains(&decl.name) {
            decl.body.retain(|stmt| !matches!(&stmt.kind, StmtKind::Section(section) if section.name == "tests"));
            continue;
        }
        let name = decl.name.clone();
        let mut index = 0;
        for stmt in &mut decl.body {
            let StmtKind::Section(section) = &mut stmt.kind else {
                continue;
            };
            if section.name != "tests" {
                continue;
            }
            for inner in &mut section.suite.statements {
                if let StmtKind::Section(example) = &mut inner.kind
                    && example.name == "example"
                {
                    example.generic = Some(format!("{name}#{index}"));
                    index += 1;
                }
            }
        }
    }
    let report = emath_exec_ir::constructor_layer::evaluate_tree_at(&tree, Some(&source.path)).map_err(|error| {
        format!("{}: emath could not run its examples: {}: {}", source.rel, error.code, error.message)
    })?;
    let mut faults = BTreeMap::new();
    for test in report.tests {
        let Some(code) = test.fault else {
            continue;
        };
        let key = match test.label.rsplit_once('#').map(|(name, index)| (name, index.parse::<usize>())) {
            Some((name, Ok(index))) => (name.to_string(), Some(index)),
            _ => (test.label, None),
        };
        faults.insert(key, code);
    }
    Ok(faults)
}

/// A law's `search.certify` call as emath source. A work limit that is
/// not a literal (the certifying function's input, say) becomes a
/// generous literal, since the call runs on its own.
fn emath_call(text: &str, call: &str, args: &[Expr]) -> String {
    let mut parts: Vec<&str> = args
        .iter()
        .map(|arg| text.get(arg.source.start as usize..arg.source.end as usize).unwrap_or(""))
        .collect();
    if let Some(limit) = parts.last_mut()
        && limit.trim().parse::<u64>().is_err()
    {
        *limit = "1000000000";
    }
    format!("{call}({})", parts.join(", "))
}

/// Whether emath certifies each law call (`LawSite::emath_call`) stated
/// in `path`, each run on its own beside the file's declarations. A law
/// emath cannot run counts as not certified.
pub(crate) fn emath_certifies(path: &Path, calls: &[&str]) -> Vec<bool> {
    let mut certified = vec![false; calls.len()];
    let Ok(text) = std::fs::read_to_string(path) else {
        return certified;
    };
    let (mut tree, _) = emath_syntax::parse_str(&text);
    for item in &mut tree.items {
        if let Item::Declaration(decl) = item {
            decl.body.retain(|stmt| !matches!(&stmt.kind, StmtKind::Section(section) if section.name == "tests"));
        }
    }
    let mut laws = String::new();
    for (k, call) in calls.iter().enumerate() {
        laws.push_str(&format!(
            "emath function emath_export_law_{k}:\n    inputs:\n        unused: Int\n    outputs:\n        result: Bool\n    definitions:\n        result = ({call}).proven\n    tests:\n        example <law_{k}>:\n            given unused = 0\n            expect result == true\n\n"
        ));
    }
    let (extra, diagnostics) = emath_syntax::parse_str(&laws);
    if diagnostics.has_errors() {
        return certified;
    }
    tree.items.extend(extra.items);
    // The call's own limit bounds each point it evaluates; this bounds
    // the whole certificate far above what a law that Lean proves needs.
    let work = 1_000_000_000_000;
    let Ok(report) = emath_exec_ir::constructor_layer::evaluate_tree_budgeted_at(&tree, Some(path), work) else {
        return certified;
    };
    for test in report.tests {
        if let Some(Ok(k)) = test.label.strip_prefix("law_").map(str::parse::<usize>)
            && k < calls.len()
        {
            certified[k] = test.passed;
        }
    }
    certified
}

/// `name.label : (let givens; let output := f args; expects) = true`,
/// proved by the kernel evaluating it.
fn theorem(
    name: &str,
    callee: &str,
    params: &[(String, TypeExpr)],
    outputs: &[(String, TypeExpr)],
    case: &Case,
    names: &mut Names,
) -> Result<String, String> {
    if case.expects.is_empty() {
        return Err("the example expects nothing".into());
    }
    let mut ctx = Ctx { names, scope: Vec::new() };
    let mut lines = Vec::new();
    let mut args = Vec::new();
    let mut scope = Vec::new();
    for (param, ty) in params {
        let (_, value) = case
            .givens
            .iter()
            .find(|(given, _)| given == param)
            .ok_or_else(|| format!("the example does not give `{param}`"))?;
        let (value, level, value_ty) = ctx.expr(value, false)?;
        match lean_type(ty, &ctx.names.objects) {
            Ok(ty) => {
                let (value, _) = fit(value, level, &value_ty, &ty)?;
                lines.push(format!("let {} : {} := {value}", ident(param), render(&ty)));
                scope.push((param.clone(), ty));
            }
            Err(_) => {
                lines.push(format!("let {} := {value}", ident(param)));
                scope.push((param.clone(), value_ty));
            }
        }
        args.push(ident(param));
    }
    ctx.scope.extend(scope);
    let call = format!("{callee}{}", args.iter().map(|arg| format!(" {arg}")).collect::<String>());
    match outputs {
        [(output, output_ty)] => {
            lines.push(format!("let {} := {call}", ident(output)));
            let ty = lean_type(output_ty, &ctx.names.objects).unwrap_or(Ty::Unknown);
            ctx.scope.push((output.clone(), ty));
        }
        _ => {
            // Not an emath name, so no given or output shadows it.
            let record = "«emath outputs»";
            lines.push(format!("let {record} := {call}"));
            for (output, output_ty) in outputs {
                lines.push(format!("let {} := {record}.{}", ident(output), ident(output)));
                let ty = lean_type(output_ty, &ctx.names.objects).unwrap_or(Ty::Unknown);
                ctx.scope.push((output.clone(), ty));
            }
        }
    }
    let mut checks = Vec::new();
    for expect in &case.expects {
        let (check, level, _) = ctx.expr(expect, false)?;
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

const CERTIFY: &[&str] = &["certify_equal", "certify_equal_from", "certify_all", "certify_all_from"];

/// The `search.certify` calls in a function's definitions, in order.
fn certify_calls<'a>(decl: &'a Declaration, names: &Names) -> Vec<(&'a str, &'a [Expr])> {
    let mut calls = Vec::new();
    for section in decl.sections().filter(|section| section.name == "definitions") {
        for stmt in &section.suite.statements {
            let StmtKind::Assign { value, .. } = &stmt.kind else {
                continue;
            };
            visit(value, &mut |expr| {
                if let ExprKind::Call { function, args } = &expr.kind
                    && let ExprKind::Path { segments, generics: None } = &function.kind
                    && let [name] = segments.as_slice()
                    && CERTIFY.contains(&name.as_str())
                    && names.library.get(name).is_some_and(|module| module == "search.certify")
                {
                    calls.push((name.as_str(), args.as_slice()));
                }
            });
        }
    }
    calls
}

/// `certify_equal(quote(function n in Int: left), quote(...: right), limit)`
/// (or `certify_equal_from` with a start) as `∀ n ≥ start, left = right`,
/// proved by bounding each side's order (`emath_order`, unfolding the
/// user functions it reaches) and checking the first points. The
/// `certify_all` forms go to `law_all`.
fn law(
    name: &str,
    call: &str,
    args: &[Expr],
    names: &mut Names,
    calls: &BTreeMap<String, BTreeSet<String>>,
    bodies: &BTreeMap<String, Body>,
) -> Result<LawText, String> {
    let start_of = |start: &Expr| match &start.kind {
        ExprKind::Int(digits) => {
            digits.replace('_', "").parse::<u64>().map_err(|_| "the start is too large".to_string())
        }
        _ => Err("the start must be a whole-number literal".to_string()),
    };
    let (left, right, start) = match (call, args) {
        ("certify_equal", [left, right, _]) => (left, right, 0),
        ("certify_equal_from", [left, right, start, _]) => (left, right, start_of(start)?),
        ("certify_all", [left, right, _]) => return law_all(name, left, right, 0, names, calls, bodies),
        ("certify_all_from", [left, right, start, _]) => {
            return law_all(name, left, right, start_of(start)?, names, calls, bodies);
        }
        ("certify_equal" | "certify_all", _) => return Err(format!("`{call}` takes two quoted functions and a limit")),
        _ => return Err(format!("`{call}` takes two quoted functions, a start and a limit")),
    };
    let mut side = |quoted: &Expr| -> Result<(String, String), String> {
        let (param, domain, body) = match &quoted.kind {
            ExprKind::Quote { body } => match &body.kind {
                ExprKind::FunctionAbs { param, domain, body } => (param, domain, body),
                _ => return Err("a side is not `quote(function n in Int: ...)`".into()),
            },
            ExprKind::QuoteBind { param, domain, body } => (param, domain, body),
            _ => return Err("a side is not `quote(function n in Int: ...)`".into()),
        };
        if domain_type(domain, &names.objects)? != Ty::Int {
            return Err("a side's variable is not over `Int`".into());
        }
        names.called.clear();
        let mut ctx = Ctx { names, scope: vec![(param.clone(), Ty::Int)] };
        let (text, level, ty) = ctx.expr(body, false)?;
        if !matches!(ty, Ty::Int | Ty::Rat | Ty::Lit) {
            return Err("a side's value is not a number".into());
        }
        let (text, _) = coerce(text, level, &ty, &Ty::Rat);
        let unfold = reached(&ctx.names.called, calls);
        Ok((format!("(fun ({} : Int) => {text}) (n₀ : Int)", ident(param)), unfold))
    };
    let (left, left_unfold) = side(left)?;
    let (right, right_unfold) = side(right)?;
    let (from, condition) = if start == 0 {
        (String::new(), String::new())
    } else {
        (format!("_from {start}"), format!(" {start} ≤ n₀ →"))
    };
    Ok(LawText {
        text: format!(
            "theorem {name} : ∀ n₀ : ℕ,{condition}\n    {left} =\n      {right} :=\n  \
             Emath.Law.eq_of_order{from}\n    (by emath_order [{left_unfold}])\n    \
             (by emath_order [{right_unfold}])\n    (by emath_check {start})\n\n"
        ),
        order: [4, 5],
        bound: Vec::new(),
        check: 6,
    })
}

/// The binders a quoted function literal opens with (name, domain),
/// and the body inside the last of them.
fn binders(quoted: &Expr) -> Result<(Vec<(&str, &Expr)>, &Expr), String> {
    let (param, domain, mut body) = match &quoted.kind {
        ExprKind::Quote { body } => match &body.kind {
            ExprKind::FunctionAbs { param, domain, body } => (param, domain.as_ref(), body.as_ref()),
            _ => return Err("a side is not a quoted `function n in Int: ...`".into()),
        },
        ExprKind::QuoteBind { param, domain, body } => (param, domain.as_ref(), body.as_ref()),
        _ => return Err("a side is not a quoted `function n in Int: ...`".into()),
    };
    let mut out = vec![(param.as_str(), domain)];
    while let ExprKind::FunctionAbs { param, domain, body: inner } = &body.kind {
        out.push((param.as_str(), domain.as_ref()));
        body = inner;
    }
    Ok((out, body))
}

/// The user functions `called` reaches, for `emath_order` to unfold.
fn reached(called: &BTreeSet<String>, calls: &BTreeMap<String, BTreeSet<String>>) -> String {
    let mut reached = BTreeSet::new();
    let mut pending: Vec<String> = called.iter().cloned().collect();
    while let Some(function) = pending.pop() {
        if reached.insert(function.clone()) {
            pending.extend(calls.get(&function).into_iter().flatten().cloned());
        }
    }
    reached.into_iter().collect::<Vec<_>>().join(", ")
}

/// `certify_all(quote(function x1 in Rat: ... function n in Int: left),
/// quote(...: right), limit)` (or `certify_all_from`) as: for all
/// rationals x1, ..., xk meeting the hypotheses, `∀ n ≥ start, left =
/// right`. emath's proof covers the parameter points where each branch
/// condition on the parameters takes the side it takes on emath's grid,
/// and where both programs compute; the hypotheses are those conditions
/// (with the side they take at the grid's first point, parameter i =
/// 1/(i + 2)) and every divisor the programs reach that reads only the
/// parameters, nonzero. Proved by `emath_order` for the orders in n and
/// `params_eq`: one kernel evaluation at a point no nonzero rational
/// function of the bounded size can vanish at (`Law.lean`). A divisor
/// that reads n or a loop value gets no hypothesis, so the proof then
/// fails rather than claim more than emath.
fn law_all(
    name: &str,
    left: &Expr,
    right: &Expr,
    start: u64,
    names: &mut Names,
    calls: &BTreeMap<String, BTreeSet<String>>,
    bodies: &BTreeMap<String, Body>,
) -> Result<LawText, String> {
    let (left_binders, left_body) = binders(left)?;
    let (right_binders, right_body) = binders(right)?;
    let k = left_binders.len() - 1;
    if k == 0 || right_binders.len() != k + 1 {
        return Err("the sides do not open with the same parameters and then n".into());
    }
    for (_, domain) in left_binders[..k].iter().chain(&right_binders[..k]) {
        if domain_type(domain, &names.objects)? != Ty::Rat {
            return Err("a parameter is not over `Rat`".into());
        }
    }
    for (_, domain) in [left_binders[k], right_binders[k]] {
        if domain_type(domain, &names.objects)? != Ty::Int {
            return Err("a side's last variable is not over `Int`".into());
        }
    }
    let params: Vec<String> = left_binders[..k].iter().map(|(param, _)| param.to_string()).collect();
    let mut used = BTreeSet::new();
    for body in [left_body, right_body] {
        visit(body, &mut |expr| {
            if let ExprKind::Path { segments, .. } = &expr.kind {
                used.extend(segments.iter().cloned());
            }
        });
    }
    let point = ["x", "y", "z", "w", "v"]
        .into_iter()
        .map(str::to_string)
        .chain((1..).map(|i| format!("x{i}")))
        .find(|candidate| {
            !used.contains(candidate) && candidate.as_str() != left_binders[k].0 && candidate.as_str() != right_binders[k].0
        })
        .expect("an unused name");
    let at = |i: usize| Expr {
        kind: ExprKind::Call {
            function: Box::new(path(&point)),
            args: vec![Expr { kind: ExprKind::Int(i.to_string()), source: Default::default() }],
        },
        source: Default::default(),
    };

    let mut reach = Reach {
        bodies,
        params: &params,
        grid: params.iter().enumerate().map(|(i, param)| (param.clone(), (1, i as i128 + 2))).collect(),
        fresh: 0,
        depth: 0,
        valued: BTreeSet::new(),
        divisors: Vec::new(),
        branches: Vec::new(),
    };
    let mut statement = Vec::new();
    let mut functions = Vec::new();
    let mut unfolds = Vec::new();
    for (binders, body) in [(&left_binders, left_body), (&right_binders, right_body)] {
        let n = binders[k].0;
        let mut reserved: BTreeSet<&str> = params.iter().map(String::as_str).collect();
        reserved.extend(binders.iter().map(|(param, _)| *param));
        reserved.insert(point.as_str());
        let mut rebinds = None;
        visit(body, &mut |expr| {
            if let ExprKind::FunctionAbs { param, .. } = &expr.kind
                && reserved.contains(param.as_str())
            {
                rebinds = Some(param.clone());
            }
        });
        if let Some(param) = rebinds {
            return Err(format!("a closure in the law rebinds `{param}`"));
        }
        let to_params: BTreeMap<String, Expr> =
            binders[..k].iter().zip(&params).map(|((own, _), param)| (own.to_string(), path(param))).collect();
        let to_point: BTreeMap<String, Expr> =
            binders[..k].iter().enumerate().map(|(i, (own, _))| (own.to_string(), at(i))).collect();

        names.called.clear();
        let mut scope: Vec<(String, Ty)> = params.iter().map(|param| (param.clone(), Ty::Rat)).collect();
        scope.push((n.to_string(), Ty::Int));
        let mut ctx = Ctx { names, scope };
        let (text, level, ty) = ctx.expr(&rename(body, &to_params), false)?;
        if !matches!(ty, Ty::Int | Ty::Rat | Ty::Lit) {
            return Err("a side's value is not a number".into());
        }
        let (text, _) = coerce(text, level, &ty, &Ty::Rat);
        statement.push(format!("(fun ({} : Int) => {text}) (n₀ : Int)", ident(n)));
        unfolds.push(reached(&ctx.names.called, calls));

        ctx.scope = vec![(point.clone(), arrows(vec![Ty::Int], Ty::Rat)), (n.to_string(), Ty::Int)];
        let (text, level, ty) = ctx.expr(&rename(body, &to_point), false)?;
        let (text, _) = coerce(text, level, &ty, &Ty::Rat);
        functions.push(format!("(fun ({} : Int) => {text}) (n : Int)", ident(n)));

        let mut env: BTreeMap<String, Expr> = to_params;
        env.insert(n.to_string(), reach.fresh());
        reach.expand(body, &env, &visible(names))?;
    }

    // Each hypothesis as the theorem states it and as `S` reads it at `x`.
    let mut hypotheses: Vec<(String, String)> = Vec::new();
    let to_point: BTreeMap<String, Expr> = params.iter().enumerate().map(|(i, param)| (param.clone(), at(i))).collect();
    let param_scope: Vec<(String, Ty)> = params.iter().map(|param| (param.clone(), Ty::Rat)).collect();
    let point_scope = vec![(point.clone(), arrows(vec![Ty::Int], Ty::Rat))];
    let facts = reach.branches.iter().map(|(condition, side)| (condition, Some(*side)));
    for (expr, side) in facts.chain(reach.divisors.iter().map(|divisor| (divisor, None))) {
        let mut state = |expr: &Expr, scope: &Vec<(String, Ty)>| -> Result<String, String> {
            let mut ctx = Ctx { names: &mut *names, scope: scope.clone() };
            Ok(match side {
                Some(true) => prop(&mut ctx, expr)?,
                Some(false) => format!("¬ ({})", prop(&mut ctx, expr)?),
                None => {
                    let (text, level, _) = ctx.expr(expr, false)?;
                    format!("{} ≠ 0", paren(text, level, LEVEL_ADD))
                }
            })
        };
        let text = state(expr, &param_scope)?;
        let at_point = state(&rename(expr, &to_point), &point_scope)?;
        if !hypotheses.iter().any(|(seen, _)| *seen == text) {
            hypotheses.push((text, at_point));
        }
    }

    let hyp_names: Vec<String> = (1..=hypotheses.len()).map(|i| format!("h{}", subscript(i))).collect();
    let set = if hypotheses.is_empty() {
        "True".to_string()
    } else {
        hypotheses
            .iter()
            .map(|(_, at_point)| {
                if at_point.contains(" ∧ ") || at_point.contains(" ∨ ") { format!("({at_point})") } else { at_point.clone() }
            })
            .collect::<Vec<_>>()
            .join(" ∧ ")
    };
    let proof = match hyp_names.as_slice() {
        [] => "trivial".to_string(),
        [one] => one.clone(),
        many => format!("⟨{}⟩", many.join(", ")),
    };
    let (condition, intro, from_start) = if start == 0 {
        (String::new(), String::new(), "(Nat.zero_le _)".to_string())
    } else {
        (format!(" {start} ≤ n₀ →"), " hn".to_string(), "hn".to_string())
    };
    let [left, right] = [&statement[0], &statement[1]];
    let [left_unfold, right_unfold] = [&unfolds[0], &unfolds[1]];
    let mut lines = vec![
        // One evaluation can outrun the default budget; the kernel still
        // checks the whole term.
        "set_option maxHeartbeats 0 in".to_string(),
        format!(
            "theorem {name} {}",
            params.iter().map(|param| format!("({} : ℚ)", ident(param))).collect::<Vec<_>>().join(" ")
        ),
    ];
    for (hyp, (text, _)) in hyp_names.iter().zip(&hypotheses) {
        lines.push(format!("    ({hyp} : {text})"));
    }
    lines.push(format!("    : ∀ n₀ : ℕ,{condition}"));
    lines.push(format!("    {left} ="));
    lines.push(format!("      {right} := fun n₀{intro} =>"));
    lines.push(format!("  Emath.Law.eq_of_order_from (u := fun n₀ : ℕ => {left})"));
    lines.push(format!("    (v := fun n₀ : ℕ => {right}) {start}"));
    let order = [lines.len(), lines.len() + 1];
    lines.push(format!("    (by emath_order [{left_unfold}])"));
    lines.push(format!("    (by emath_order [{right_unfold}])"));
    lines.push(format!("    (Emath.Law.params_eq (fun {point} : Fin {k} → ℚ => {set})"));
    lines.push(format!("      (fun {point} (n : ℕ) => {})", functions[0]));
    lines.push(format!("      (fun {point} (n : ℕ) => {}) _ _", functions[1]));
    let bound = vec![lines.len(), lines.len() + 1];
    lines.push(format!("      (fun n => by emath_bound [{left_unfold}])"));
    lines.push(format!("      (fun n => by emath_bound [{right_unfold}])"));
    let check = lines.len();
    lines.push(format!(
        "      {start} _ (by emath_kcheck) ![{}] {proof})",
        params.iter().map(|param| ident(param)).collect::<Vec<_>>().join(", ")
    ));
    lines.push(format!("    n₀ {from_start}"));
    Ok(LawText { text: format!("{}\n\n", lines.join("\n")), order, bound, check })
}

/// The qualified Lean name of each user function `names` can call.
fn visible(names: &Names) -> BTreeMap<String, String> {
    names
        .targets
        .iter()
        .filter_map(|(local, target)| match target {
            Target::Lean(lean, _) if lean.contains('.') => Some((local.clone(), lean.clone())),
            Target::Lean(lean, _) => Some((local.clone(), format!("{}.{lean}", names.namespace))),
            Target::Fuel(..) | Target::Refused(_) => None,
        })
        .collect()
}

fn subscript(i: usize) -> String {
    i.to_string().chars().map(|digit| char::from_u32(0x2080 + digit.to_digit(10).unwrap_or(0)).unwrap_or('_')).collect()
}

fn path(name: &str) -> Expr {
    Expr { kind: ExprKind::Path { segments: vec![name.to_string()], generics: None }, source: Default::default() }
}

/// `expr` with each name in `map` replaced; the caller makes sure no
/// closure inside rebinds one of them.
fn rename(expr: &Expr, map: &BTreeMap<String, Expr>) -> Expr {
    let go = |inner: &Expr| Box::new(rename(inner, map));
    let kind = match &expr.kind {
        ExprKind::Path { segments, generics: None } if segments.len() == 1 && map.contains_key(&segments[0]) => {
            return map[&segments[0]].clone();
        }
        ExprKind::Call { function, args } => {
            ExprKind::Call { function: go(function), args: args.iter().map(|arg| rename(arg, map)).collect() }
        }
        ExprKind::Index { value, indices } => {
            ExprKind::Index { value: go(value), indices: indices.iter().map(|index| rename(index, map)).collect() }
        }
        ExprKind::Unary { op, value } => ExprKind::Unary { op: *op, value: go(value) },
        ExprKind::Binary { op, left, right } => ExprKind::Binary { op: *op, left: go(left), right: go(right) },
        ExprKind::If { condition, then_value, else_value } => ExprKind::If {
            condition: go(condition),
            then_value: go(then_value),
            else_value: go(else_value),
        },
        ExprKind::List(items) => ExprKind::List(items.iter().map(|item| rename(item, map)).collect()),
        ExprKind::SequenceCons { head, tail, spread_head } => {
            ExprKind::SequenceCons { head: go(head), tail: go(tail), spread_head: *spread_head }
        }
        ExprKind::FunctionAbs { param, domain, body } => {
            ExprKind::FunctionAbs { param: param.clone(), domain: domain.clone(), body: go(body) }
        }
        other => other.clone(),
    };
    Expr { kind, source: expr.source }
}

/// A Boolean condition as a Lean proposition: comparisons as relations,
/// `and`, `or` and `not` as connectives, anything else as `= true`.
fn prop(ctx: &mut Ctx, expr: &Expr) -> Result<String, String> {
    let relation = match &expr.kind {
        ExprKind::Binary { op: BinaryOp::And, left, right } => {
            return Ok(format!("({}) ∧ ({})", prop(ctx, left)?, prop(ctx, right)?));
        }
        ExprKind::Binary { op: BinaryOp::Or, left, right } => {
            return Ok(format!("({}) ∨ ({})", prop(ctx, left)?, prop(ctx, right)?));
        }
        ExprKind::Unary { op: UnaryOp::Not, value } => return Ok(format!("¬ ({})", prop(ctx, value)?)),
        ExprKind::Bool(value) => return Ok(if *value { "True" } else { "False" }.to_string()),
        ExprKind::Binary { op, left, right } => match op {
            BinaryOp::Eq => Some(("=", left, right)),
            BinaryOp::Ne => Some(("≠", left, right)),
            BinaryOp::Lt => Some(("<", left, right)),
            BinaryOp::Le => Some(("≤", left, right)),
            BinaryOp::Gt => Some((">", left, right)),
            BinaryOp::Ge => Some(("≥", left, right)),
            _ => None,
        },
        _ => None,
    };
    match relation {
        Some((symbol, left, right)) => {
            let (l, _) = ctx.operand(left, is_literal(left) && !is_literal(right), LEVEL_CONS)?;
            let (r, _) = ctx.operand(right, is_literal(right) && !is_literal(left), LEVEL_CONS)?;
            Ok(format!("{l} {symbol} {r}"))
        }
        None => {
            let (text, level, _) = ctx.expr(expr, false)?;
            Ok(format!("{} = true", paren(text, level, LEVEL_APP)))
        }
    }
}

/// Reads the divisors and branch conditions a law's sides reach through
/// the user functions they call, as expressions in the law's
/// parameters. A value that depends on anything else (n, a closure's
/// variable, a field) becomes a fresh name `#k`, so it is never mistaken
/// for one that reads only the parameters.
struct Reach<'a> {
    bodies: &'a BTreeMap<String, Body>,
    params: &'a [String],
    /// The first point of emath's grid.
    grid: BTreeMap<String, (i128, i128)>,
    fresh: usize,
    depth: usize,
    /// Functions already read as values (passed, not called).
    valued: BTreeSet<String>,
    divisors: Vec<Expr>,
    branches: Vec<(Expr, bool)>,
}

/// Past this many nodes a definition's value is not carried into the
/// expressions that use it.
const REACH_NODES: usize = 4000;

impl Reach<'_> {
    fn fresh_name(&mut self) -> String {
        self.fresh += 1;
        format!("#{}", self.fresh)
    }

    fn fresh(&mut self) -> Expr {
        path(&self.fresh_name())
    }

    /// Reads only the parameters (and prelude functions).
    fn closed_over_params(&self, expr: &Expr) -> bool {
        let mut ok = true;
        visit(expr, &mut |inner| match &inner.kind {
            ExprKind::Path { segments, generics: None } => {
                let [name] = segments.as_slice() else {
                    ok = false;
                    return;
                };
                if !self.params.contains(name) && prelude_signature(name).is_none() {
                    ok = false;
                }
            }
            ExprKind::Path { .. } | ExprKind::FunctionAbs { .. } => ok = false,
            _ => {}
        });
        ok
    }

    fn reads_params(&self, expr: &Expr) -> bool {
        let mut reads = false;
        visit(expr, &mut |inner| {
            if let ExprKind::Path { segments, .. } = &inner.kind
                && segments.len() == 1
                && self.params.contains(&segments[0])
            {
                reads = true;
            }
        });
        reads
    }

    fn expand(
        &mut self,
        expr: &Expr,
        env: &BTreeMap<String, Expr>,
        names: &BTreeMap<String, String>,
    ) -> Result<Expr, String> {
        let kind = match &expr.kind {
            ExprKind::Int(_) | ExprKind::Rational { .. } | ExprKind::Bool(_) => return Ok(expr.clone()),
            ExprKind::Path { segments, generics: None } if segments.len() == 1 => {
                let name = &segments[0];
                if let Some(value) = env.get(name) {
                    return Ok(value.clone());
                }
                if let Some(qualified) = names.get(name) {
                    self.function_value(qualified)?;
                    return Ok(self.fresh());
                }
                return Ok(expr.clone());
            }
            ExprKind::Path { .. } => return Ok(self.fresh()),
            ExprKind::Call { function, args } => {
                let args = args.iter().map(|arg| self.expand(arg, env, names)).collect::<Result<Vec<_>, _>>()?;
                if let ExprKind::Path { segments, generics: None } = &function.kind
                    && let [name] = segments.as_slice()
                    && !env.contains_key(name)
                    && let Some(qualified) = names.get(name)
                {
                    return self.inline(qualified, args);
                }
                ExprKind::Call { function: Box::new(self.expand(function, env, names)?), args }
            }
            ExprKind::Index { value, indices } => ExprKind::Index {
                value: Box::new(self.expand(value, env, names)?),
                indices: indices.iter().map(|index| self.expand(index, env, names)).collect::<Result<_, _>>()?,
            },
            ExprKind::Unary { op, value } => ExprKind::Unary { op: *op, value: Box::new(self.expand(value, env, names)?) },
            ExprKind::Binary { op, left, right } => {
                let left = self.expand(left, env, names)?;
                let right = self.expand(right, env, names)?;
                if *op == BinaryOp::Div && self.closed_over_params(&right) && self.reads_params(&right) {
                    self.divisors.push(right.clone());
                }
                ExprKind::Binary { op: *op, left: Box::new(left), right: Box::new(right) }
            }
            ExprKind::If { condition, then_value, else_value } => {
                let condition = self.expand(condition, env, names)?;
                if self.closed_over_params(&condition) {
                    let Some(Val::Bool(side)) = eval(&condition, &self.grid) else {
                        return Err("a branch condition on the parameters cannot be evaluated at emath's first grid point".into());
                    };
                    if self.reads_params(&condition) {
                        self.branches.push((condition, side));
                    }
                    return self.expand(if side { then_value } else { else_value }, env, names);
                }
                ExprKind::If {
                    condition: Box::new(condition),
                    then_value: Box::new(self.expand(then_value, env, names)?),
                    else_value: Box::new(self.expand(else_value, env, names)?),
                }
            }
            ExprKind::List(items) => {
                ExprKind::List(items.iter().map(|item| self.expand(item, env, names)).collect::<Result<_, _>>()?)
            }
            ExprKind::SequenceCons { head, tail, spread_head } => ExprKind::SequenceCons {
                head: Box::new(self.expand(head, env, names)?),
                tail: Box::new(self.expand(tail, env, names)?),
                spread_head: *spread_head,
            },
            ExprKind::FunctionAbs { param, domain, body } => {
                let fresh = self.fresh_name();
                let mut inner = env.clone();
                inner.insert(param.clone(), path(&fresh));
                let body = self.expand(body, &inner, names)?;
                ExprKind::FunctionAbs { param: fresh, domain: domain.clone(), body: Box::new(body) }
            }
            other => return Err(format!("{} has no Lean translation", construct(other))),
        };
        Ok(Expr { kind, source: expr.source })
    }

    /// The output of user function `qualified` at `args`.
    fn inline(&mut self, qualified: &str, args: Vec<Expr>) -> Result<Expr, String> {
        let bodies = self.bodies;
        let Some(body) = bodies.get(qualified) else {
            return Err(format!("the law reaches `{qualified}`, whose definition is not translated"));
        };
        let params = section_typed_fields(&body.decl, "inputs");
        if params.len() != args.len() {
            return Ok(self.fresh());
        }
        self.depth += 1;
        if self.depth > 64 {
            return Err("calls nest too deep to read the law's divisors".into());
        }
        let mut env: BTreeMap<String, Expr> = params.into_iter().map(|(name, _)| name).zip(args).collect();
        for section in body.decl.sections().filter(|section| section.name == "definitions") {
            for stmt in &section.suite.statements {
                if let StmtKind::Assign { target, value } = &stmt.kind
                    && let [name] = target.segments.as_slice()
                {
                    let mut value = self.expand(value, &env, &body.names)?;
                    let mut nodes = 0;
                    visit(&value, &mut |_| nodes += 1);
                    if nodes > REACH_NODES {
                        value = self.fresh();
                    }
                    env.insert(name.clone(), value);
                }
            }
        }
        self.depth -= 1;
        // Several outputs make a record, which no divisor or condition
        // reads whole; its fields read as unknown values.
        let outputs = section_typed_fields(&body.decl, "outputs");
        let output = (outputs.len() == 1).then(|| outputs[0].0.clone());
        Ok(match output.and_then(|name| env.remove(&name)) {
            Some(value) => value,
            None => self.fresh(),
        })
    }

    /// Reads the divisors and conditions of a function passed as a value.
    fn function_value(&mut self, qualified: &str) -> Result<(), String> {
        if !self.valued.insert(qualified.to_string()) {
            return Ok(());
        }
        let bodies = self.bodies;
        let Some(body) = bodies.get(qualified) else {
            return Err(format!("the law reaches `{qualified}`, whose definition is not translated"));
        };
        let count = section_typed_fields(&body.decl, "inputs").len();
        let args = (0..count).map(|_| self.fresh()).collect();
        self.inline(qualified, args).map(|_| ())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Val {
    /// numerator, denominator > 0, in lowest terms.
    Num(i128, i128),
    Bool(bool),
}

fn ratio(numer: i128, denom: i128) -> Option<Val> {
    if denom == 0 {
        return None;
    }
    let (mut a, mut b) = (numer.unsigned_abs(), denom.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let g = i128::try_from(a.max(1)).ok()?;
    let sign = if denom < 0 { -1 } else { 1 };
    Some(Val::Num(sign * numer / g, sign * denom / g))
}

/// `expr` at `point`, exactly; `None` past `i128` or outside the
/// arithmetic and logic of numbers.
fn eval(expr: &Expr, point: &BTreeMap<String, (i128, i128)>) -> Option<Val> {
    let num = |expr: &Expr| match eval(expr, point)? {
        Val::Num(n, d) => Some((n, d)),
        Val::Bool(_) => None,
    };
    match &expr.kind {
        ExprKind::Int(digits) => ratio(digits.replace('_', "").parse().ok()?, 1),
        ExprKind::Rational { numer, denom } => {
            ratio(numer.replace('_', "").parse().ok()?, denom.replace('_', "").parse().ok()?)
        }
        ExprKind::Bool(value) => Some(Val::Bool(*value)),
        ExprKind::Path { segments, generics: None } if segments.len() == 1 => {
            point.get(&segments[0]).and_then(|&(n, d)| ratio(n, d))
        }
        ExprKind::Unary { op: UnaryOp::Pos, value } => eval(value, point),
        ExprKind::Unary { op: UnaryOp::Neg, value } => {
            let (n, d) = num(value)?;
            ratio(n.checked_neg()?, d)
        }
        ExprKind::Unary { op: UnaryOp::Not, value } => match eval(value, point)? {
            Val::Bool(value) => Some(Val::Bool(!value)),
            Val::Num(..) => None,
        },
        ExprKind::Binary { op: BinaryOp::And | BinaryOp::Or, left, right } => {
            let (Val::Bool(l), Val::Bool(r)) = (eval(left, point)?, eval(right, point)?) else {
                return None;
            };
            let and = matches!(&expr.kind, ExprKind::Binary { op: BinaryOp::And, .. });
            Some(Val::Bool(if and { l && r } else { l || r }))
        }
        ExprKind::Binary { op, left, right } => {
            let ((a, b), (c, d)) = (num(left)?, num(right)?);
            let cross = || Some((a.checked_mul(d)?, c.checked_mul(b)?));
            match op {
                BinaryOp::Add => ratio(a.checked_mul(d)?.checked_add(c.checked_mul(b)?)?, b.checked_mul(d)?),
                BinaryOp::Sub => ratio(a.checked_mul(d)?.checked_sub(c.checked_mul(b)?)?, b.checked_mul(d)?),
                BinaryOp::Mul => ratio(a.checked_mul(c)?, b.checked_mul(d)?),
                BinaryOp::Div => ratio(a.checked_mul(d)?, b.checked_mul(c)?),
                BinaryOp::Eq => Some(Val::Bool(a == c && b == d)),
                BinaryOp::Ne => Some(Val::Bool(a != c || b != d)),
                BinaryOp::Lt => cross().map(|(l, r)| Val::Bool(l < r)),
                BinaryOp::Le => cross().map(|(l, r)| Val::Bool(l <= r)),
                BinaryOp::Gt => cross().map(|(l, r)| Val::Bool(l > r)),
                BinaryOp::Ge => cross().map(|(l, r)| Val::Bool(l >= r)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `f` on `expr` and every expression inside it, outermost first.
fn visit<'a>(expr: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(expr);
    match &expr.kind {
        ExprKind::Call { function, args } => {
            visit(function, f);
            args.iter().for_each(|arg| visit(arg, f));
        }
        ExprKind::Index { value, indices } => {
            visit(value, f);
            indices.iter().for_each(|index| visit(index, f));
        }
        ExprKind::Unary { value, .. } => visit(value, f),
        ExprKind::Binary { left, right, .. } => {
            visit(left, f);
            visit(right, f);
        }
        ExprKind::If { condition, then_value, else_value } => {
            visit(condition, f);
            visit(then_value, f);
            visit(else_value, f);
        }
        ExprKind::List(items) => items.iter().for_each(|item| visit(item, f)),
        ExprKind::FunctionAbs { body, .. } | ExprKind::Quote { body } | ExprKind::QuoteBind { body, .. } => {
            visit(body, f)
        }
        ExprKind::SequenceCons { head, tail, .. } => {
            visit(head, f);
            visit(tail, f);
        }
        _ => {}
    }
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
        // A library declaration named like a machine operation is never
        // what a call by that name runs, so its examples say nothing
        // about the prelude's machine operation.
        for decl in functions(&libraries[file]).filter(|decl| used.contains(&decl.name) && !machine_int(&decl.name)) {
            let params = section_typed_fields(decl, "inputs");
            let outputs = section_typed_fields(decl, "outputs");
            if outputs.len() != 1 {
                continue;
            }
            for case in examples(decl) {
                let mut names = Names::default();
                let callee = format!("Emath.{}", decl.name);
                if let Ok(theorem) = theorem(&decl.name, &callee, &params, &outputs, &case, &mut names) {
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

type Out = (String, u8, Ty);

struct Ctx<'a> {
    names: &'a mut Names,
    scope: Vec<(String, Ty)>,
}

impl Ctx<'_> {
    fn local(&self, name: &str) -> Option<Ty> {
        self.scope.iter().rev().find(|(bound, _)| bound == name).map(|(_, ty)| ty.clone())
    }

    /// Lean text, its binding level and its type. `bare`: an integer
    /// literal may stay untyped because a typed neighbour fixes its type.
    fn expr(&mut self, expr: &Expr, bare: bool) -> Result<Out, String> {
        if !bare && is_literal(expr) && !has_division(expr) {
            let (text, _, _) = self.expr(expr, true)?;
            return Ok((format!("({text} : Int)"), LEVEL_ATOM, Ty::Int));
        }
        match &expr.kind {
            ExprKind::Int(digits) => Ok((digits.replace('_', ""), LEVEL_ATOM, Ty::Lit)),
            ExprKind::Rational { numer, denom } => Ok((
                format!("(({} : Rat) / {})", numer.replace('_', ""), denom.replace('_', "")),
                LEVEL_ATOM,
                Ty::Rat,
            )),
            ExprKind::Bool(value) => Ok((value.to_string(), LEVEL_ATOM, Ty::Bool)),
            ExprKind::Path { segments, generics: None } => self.path(segments),
            ExprKind::Call { function, args } => self.call(function, args),
            ExprKind::Index { value, indices } => {
                let [index] = indices.as_slice() else {
                    return Err("indexing with several indices has no Lean translation".into());
                };
                let (value, ty) = self.operand(value, false, LEVEL_ATOM)?;
                let (index, _) = self.operand(index, true, LEVEL_ATOM)?;
                let element = match ty {
                    Ty::List(element) => *element,
                    _ => Ty::Unknown,
                };
                Ok((format!("Emath.index {value} {index}"), LEVEL_APP, element))
            }
            ExprKind::Unary { op: UnaryOp::Pos, value } => self.expr(value, bare),
            ExprKind::Unary { op: UnaryOp::Neg, value } => {
                let (value, ty) = self.operand(value, bare || is_literal(value), LEVEL_ATOM)?;
                if has_param(&ty) {
                    return Err("arithmetic on a generic value has no Lean translation".into());
                }
                Ok((format!("-{value}"), LEVEL_APP, ty))
            }
            ExprKind::Unary { op: UnaryOp::Not, value } => {
                let (value, _) = self.operand(value, false, LEVEL_ATOM)?;
                Ok((format!("!{value}"), LEVEL_APP, Ty::Bool))
            }
            ExprKind::Binary { op, left, right } => self.binary(*op, left, right, bare),
            ExprKind::If { condition, then_value, else_value } => {
                let (condition, _, _) = self.expr(condition, false)?;
                let then_bare = is_literal(then_value) && !is_literal(else_value);
                let else_bare = is_literal(else_value) && !is_literal(then_value);
                let (then_text, then_level, then_ty) = self.expr(then_value, then_bare)?;
                let (else_text, else_level, else_ty) = self.expr(else_value, else_bare)?;
                let ty = join(&then_ty, &else_ty);
                let (then_text, _) = coerce(then_text, then_level, &then_ty, &ty);
                let (else_text, _) = coerce(else_text, else_level, &else_ty, &ty);
                Ok((format!("if {condition} then {then_text} else {else_text}"), LEVEL_OPEN, ty))
            }
            ExprKind::List(items) => {
                let all_literal = items.iter().all(is_literal);
                let mut parts = Vec::new();
                for item in items {
                    parts.push(self.expr(item, !all_literal && is_literal(item))?);
                }
                let element = parts.iter().fold(Ty::Lit, |acc, (_, _, ty)| join(&acc, ty));
                let parts: Vec<String> =
                    parts.into_iter().map(|(text, level, ty)| coerce(text, level, &ty, &element).0).collect();
                Ok((format!("[{}]", parts.join(", ")), LEVEL_ATOM, list(element)))
            }
            ExprKind::SequenceCons { head, tail, spread_head } => {
                let (head_text, head_level, head_ty) = self.expr(head, false)?;
                let (tail_text, tail_level, tail_ty) = self.expr(tail, false)?;
                let ty = if *spread_head {
                    join(&head_ty, &tail_ty)
                } else {
                    let element = match &tail_ty {
                        Ty::List(element) => element.as_ref().clone(),
                        _ => Ty::Unknown,
                    };
                    list(join(&head_ty, &element))
                };
                let (head_text, head_level) = if *spread_head {
                    coerce(head_text, head_level, &head_ty, &ty)
                } else {
                    let Ty::List(element) = &ty else { unreachable!() };
                    coerce(head_text, head_level, &head_ty, element)
                };
                let (tail_text, tail_level) = coerce(tail_text, tail_level, &tail_ty, &ty);
                let op = if *spread_head { "++" } else { "::" };
                Ok((
                    format!(
                        "{} {op} {}",
                        paren(head_text, head_level, LEVEL_APP),
                        paren(tail_text, tail_level, LEVEL_APP)
                    ),
                    LEVEL_CONS,
                    ty,
                ))
            }
            ExprKind::FunctionAbs { param, domain, body } => {
                let ty = domain_type(domain, &self.names.objects)?;
                self.scope.push((param.clone(), ty.clone()));
                let body = self.expr(body, false);
                self.scope.pop();
                let (body, _, body_ty) = body?;
                Ok((
                    format!("fun ({} : {}) => {body}", ident(param), render(&ty)),
                    LEVEL_OPEN,
                    Ty::Fn(Box::new(ty), Box::new(body_ty)),
                ))
            }
            ExprKind::Record { type_path, fields } => {
                let name = type_path.join(".");
                let Some(Ty::Record(lean, declared)) = self.names.objects.get(&name).cloned() else {
                    return Err(format!("type `{name}` has no Lean translation"));
                };
                if fields.len() != declared.len() {
                    return Err(format!("a `{name}` record that does not give each field once has no Lean translation"));
                }
                let mut parts = Vec::new();
                for (field, field_ty) in &declared {
                    let Some((_, value)) = fields.iter().find(|(given, _)| given == field) else {
                        return Err(format!("a `{name}` record without field `{field}` has no Lean translation"));
                    };
                    let (text, level, ty) = if is_literal(value) && !has_division(value) && matches!(field_ty, Ty::Int | Ty::Rat)
                    {
                        let (text, _, _) = self.expr(value, true)?;
                        (format!("({text} : {})", render(field_ty)), LEVEL_ATOM, field_ty.clone())
                    } else {
                        self.expr(value, false)?
                    };
                    parts.push(format!("{} := {}", ident(field), fit(text, level, &ty, field_ty)?.0));
                }
                Ok((format!("({{ {} }} : {lean})", parts.join(", ")), LEVEL_ATOM, Ty::Record(lean, declared)))
            }
            other => Err(format!("{} has no Lean translation", construct(other))),
        }
    }

    /// `expr` as an operand: parenthesized when it binds looser than
    /// `max`.
    fn operand(&mut self, expr: &Expr, bare: bool, max: u8) -> Result<(String, Ty), String> {
        let (text, level, ty) = self.expr(expr, bare)?;
        Ok((paren(text, level, max), ty))
    }

    fn path(&mut self, segments: &[String]) -> Result<Out, String> {
        match segments {
            [name] => match self.local(name) {
                Some(ty) => Ok((ident(name), LEVEL_ATOM, ty)),
                None => {
                    let (text, ty) = self.names.resolve(name)?;
                    let level = if text.contains(' ') { LEVEL_APP } else { LEVEL_ATOM };
                    Ok((text, level, ty))
                }
            },
            [value, path @ ..] if let Some(Ty::Record(..)) = self.local(value) => {
                let mut text = ident(value);
                let mut ty = self.local(value).expect("a record local");
                for (k, field) in path.iter().enumerate() {
                    match (&ty, field.as_str()) {
                        (Ty::Int | Ty::Rat, "numer" | "denom") if k + 1 == path.len() => {
                            let (text, level) = coerce(text, LEVEL_ATOM, &ty, &Ty::Rat);
                            return Ok((format!("Emath.{field} {}", paren(text, level, LEVEL_ATOM)), LEVEL_APP, Ty::Int));
                        }
                        (Ty::List(_), "length") if k + 1 == path.len() => {
                            return Ok((format!("Emath.length {text}"), LEVEL_APP, Ty::Int));
                        }
                        _ => {}
                    }
                    let Ty::Record(_, fields) = &ty else {
                        return Err(format!("field `.{field}` has no Lean translation"));
                    };
                    let Some((_, field_ty)) = fields.iter().find(|(name, _)| name == field) else {
                        return Err(format!("field `.{field}` has no Lean translation"));
                    };
                    text = format!("{text}.{}", ident(field));
                    ty = field_ty.clone();
                }
                Ok((text, LEVEL_ATOM, ty))
            }
            [value, field] if self.local(value).is_some() => match field.as_str() {
                "numer" | "denom" | "length" => {
                    Ok((format!("Emath.{field} {}", ident(value)), LEVEL_APP, Ty::Int))
                }
                _ => Err(format!("field `.{field}` has no Lean translation")),
            },
            _ => Err(format!("`{}` has no Lean translation", segments.join("."))),
        }
    }

    /// Each argument is cast to its parameter's type; a generic
    /// parameter takes the type its first argument fixes.
    fn call(&mut self, function: &Expr, args: &[Expr]) -> Result<Out, String> {
        // emath faults here. An example that reaches it is not exported
        // and a law must hold in emath too, so Lean's total stand-in is
        // never compared with an emath value.
        if let ExprKind::Path { segments, generics: None } = &function.kind
            && let [name] = segments.as_slice()
            && name == "constructor_refuse"
            && self.local(name).is_none()
            && !self.names.targets.contains_key(name)
        {
            return Ok(("default".into(), LEVEL_ATOM, Ty::Unknown));
        }
        // `value.field` on a value that is not a name parses as
        // `field(value)`. emath projects the field unless `field` names
        // a function; a local closure is applied only when the value
        // has no such field.
        if let ExprKind::Path { segments, generics: None } = &function.kind
            && let [name] = segments.as_slice()
            && let [value] = args
            && (self.local(name).is_some()
                || !(self.names.targets.contains_key(name)
                    || prelude_signature(name).is_some()
                    || self.names.library.contains_key(name)))
        {
            let called = self.names.called.clone();
            let (text, level, ty) = self.expr(value, false)?;
            if let Ty::Record(_, fields) = &ty
                && let Some((_, field_ty)) = fields.iter().find(|(field, _)| field == name)
            {
                return Ok((format!("{}.{}", paren(text, level, LEVEL_ATOM), ident(name)), LEVEL_ATOM, field_ty.clone()));
            }
            match (&ty, name.as_str()) {
                (Ty::List(_), "length") => {
                    return Ok((format!("Emath.length {}", paren(text, level, LEVEL_ATOM)), LEVEL_APP, Ty::Int));
                }
                (Ty::Int | Ty::Rat, "numer" | "denom") => {
                    let (text, level) = coerce(text, level, &ty, &Ty::Rat);
                    return Ok((format!("Emath.{name} {}", paren(text, level, LEVEL_ATOM)), LEVEL_APP, Ty::Int));
                }
                _ => {}
            }
            self.names.called = called;
        }
        let (mut text, mut signature) = match &function.kind {
            ExprKind::Path { segments, generics: None } if segments.len() == 1 => {
                let (text, _, ty) = self.path(segments)?;
                (text, ty)
            }
            _ => self.operand(function, false, LEVEL_APP)?,
        };
        let mut bound = BTreeMap::new();
        for arg in args {
            let (param, rest) = match signature {
                Ty::Fn(param, rest) => (*param, *rest),
                _ => (Ty::Unknown, Ty::Unknown),
            };
            let want = substitute(&param, &bound);
            let (arg_text, level, ty) = if is_literal(arg) && !has_division(arg) {
                match &want {
                    Ty::Int | Ty::Rat if want == param => self.expr(arg, true)?,
                    Ty::Int | Ty::Rat => {
                        let (text, _, _) = self.expr(arg, true)?;
                        (format!("({text} : {})", render(&want)), LEVEL_ATOM, want.clone())
                    }
                    _ => self.expr(arg, false)?,
                }
            } else {
                self.expr(arg, false)?
            };
            unify(&param, &ty, &mut bound);
            let (arg_text, level) = fit(arg_text, level, &ty, &substitute(&param, &bound))?;
            text.push(' ');
            text.push_str(&paren(arg_text, level, LEVEL_ATOM));
            signature = rest;
        }
        Ok((text, LEVEL_APP, substitute(&signature, &bound)))
    }

    fn binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, bare: bool) -> Result<Out, String> {
        // A literal operand beside a typed one takes that type; inside
        // a literal the caller already typed, it stays bare. Lean's
        // arithmetic and comparisons insert the `Int` to `Rat` cast
        // themselves.
        let left_bare = is_literal(left) && (bare || !is_literal(right));
        let right_bare = is_literal(right) && (bare || !is_literal(left));
        let arithmetic = |ctx: &mut Self, symbol: &str, level: u8| -> Result<Out, String> {
            let (l, left_ty) = ctx.operand(left, left_bare, level)?;
            let (r, right_ty) = ctx.operand(right, right_bare, level - 1)?;
            if has_param(&left_ty) || has_param(&right_ty) {
                return Err("arithmetic on a generic value has no Lean translation".into());
            }
            Ok((format!("{l} {symbol} {r}"), level, join(&left_ty, &right_ty)))
        };
        // Lean casts an `Int` operand to `Rat` itself, but not inside a
        // list or a record.
        let relation = |ctx: &mut Self, symbol: &str| -> Result<Out, String> {
            let (l, l_level, l_ty) = ctx.expr(left, left_bare)?;
            let (r, r_level, r_ty) = ctx.expr(right, right_bare)?;
            let ty = join(&l_ty, &r_ty);
            if !decidable(&ty) || !decidable(&l_ty) || !decidable(&r_ty) {
                return Err("comparing functions or generic values has no Lean translation".into());
            }
            let (l, l_level) = if matches!(ty, Ty::List(_) | Ty::Record(..)) { coerce(l, l_level, &l_ty, &ty) } else { (l, l_level) };
            let (r, r_level) = if matches!(ty, Ty::List(_) | Ty::Record(..)) { coerce(r, r_level, &r_ty, &ty) } else { (r, r_level) };
            Ok((
                format!("decide ({} {symbol} {})", paren(l, l_level, LEVEL_CONS), paren(r, r_level, LEVEL_CONS)),
                LEVEL_APP,
                Ty::Bool,
            ))
        };
        match op {
            BinaryOp::Add => arithmetic(self, "+", LEVEL_ADD),
            BinaryOp::Sub => arithmetic(self, "-", LEVEL_ADD),
            BinaryOp::Mul => arithmetic(self, "*", LEVEL_MUL),
            BinaryOp::Div => {
                // emath `/` is exact rational division, whatever the
                // operands' types.
                let (l, l_level, l_ty) = self.expr(left, is_literal(left) && !has_division(left))?;
                let l = if l_ty == Ty::Int { coerce(l, l_level, &l_ty, &Ty::Rat).0 } else { format!("({l} : Rat)") };
                let (r, r_ty) = self.operand(right, is_literal(right), LEVEL_ATOM.max(LEVEL_MUL - 1))?;
                if has_param(&l_ty) || has_param(&r_ty) {
                    return Err("arithmetic on a generic value has no Lean translation".into());
                }
                Ok((format!("{l} / {r}"), LEVEL_MUL, Ty::Rat))
            }
            BinaryOp::Eq => relation(self, "="),
            BinaryOp::Ne => relation(self, "≠"),
            BinaryOp::Lt => relation(self, "<"),
            BinaryOp::Le => relation(self, "≤"),
            BinaryOp::Gt => relation(self, ">"),
            BinaryOp::Ge => relation(self, "≥"),
            BinaryOp::And => {
                let (l, _) = self.operand(left, false, LEVEL_AND)?;
                let (r, _) = self.operand(right, false, LEVEL_AND - 1)?;
                Ok((format!("{l} && {r}"), LEVEL_AND, Ty::Bool))
            }
            BinaryOp::Or => {
                let (l, _) = self.operand(left, false, LEVEL_OR)?;
                let (r, _) = self.operand(right, false, LEVEL_OR - 1)?;
                Ok((format!("{l} || {r}"), LEVEL_OR, Ty::Bool))
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

fn lean_type(ty: &TypeExpr, objects: &BTreeMap<String, Ty>) -> Result<Ty, String> {
    match &ty.kind {
        TypeKind::Path { segments, generic_args } => {
            let name = segments.last().map(String::as_str).unwrap_or("");
            match (name, generic_args.as_slice()) {
                ("Int" | "Nat", []) => Ok(Ty::Int),
                ("Rat", []) => Ok(Ty::Rat),
                ("Bool", []) => Ok(Ty::Bool),
                ("sequence", [GenericArg::Type(element)]) => Ok(list(lean_type(element, objects)?)),
                (_, []) if segments.len() == 1 && let Some(ty) = objects.get(name) => Ok(ty.clone()),
                _ => Err(format!("type `{}` has no Lean translation", segments.join("."))),
            }
        }
        TypeKind::Fn { domain, codomain } => Ok(Ty::Fn(
            Box::new(lean_type(domain, objects)?),
            Box::new(lean_type(codomain, objects)?),
        )),
        _ => Err("this type has no Lean translation".into()),
    }
}

/// The type a closure's `in` domain names.
fn domain_type(domain: &Expr, objects: &BTreeMap<String, Ty>) -> Result<Ty, String> {
    match &domain.kind {
        ExprKind::Path { segments, generics } => {
            let name = segments.last().map(String::as_str).unwrap_or("");
            match (name, generics.as_deref()) {
                ("Int" | "Nat", None) => Ok(Ty::Int),
                ("Rat", None) => Ok(Ty::Rat),
                ("Bool", None) => Ok(Ty::Bool),
                ("sequence", Some([GenericArg::Type(element)])) => Ok(list(lean_type(element, objects)?)),
                (_, None) if segments.len() == 1 && let Some(ty) = objects.get(name) => Ok(ty.clone()),
                _ => Err(format!("domain `{}` has no Lean translation", segments.join("."))),
            }
        }
        ExprKind::Call { function: head, args: elements } | ExprKind::Index { value: head, indices: elements } => {
            match (&head.kind, elements.as_slice()) {
                (ExprKind::Path { segments, .. }, [element]) if segments.len() == 1 && segments[0] == "sequence" => {
                    Ok(list(domain_type(element, objects)?))
                }
                _ => Err("this domain has no Lean translation".into()),
            }
        }
        _ => Err("this domain has no Lean translation".into()),
    }
}

fn atom_type(ty: String) -> String {
    if ty.contains(' ') { format!("({ty})") } else { ty }
}

/// The innermost reason of a chain of refused calls.
fn root_cause(reason: &str) -> &str {
    reason.rsplit_once(", which does not translate: ").map_or(reason, |(_, root)| root)
}

/// Names the translation itself writes unqualified: an emath name
/// spelled as one would shadow it, so it takes a `'` (which no emath
/// name contains).
const LEAN_RESERVED: &[&str] = &[
    "decide", "default", "List", "Int", "Rat", "Bool", "Nat", "Type", "true", "false", "Inhabited",
    "DecidableEq", "Emath", "Export",
];

/// A Lean identifier for an emath name: keywords and anything Lean
/// would not read as one plain identifier go inside `«»`.
fn ident(name: &str) -> String {
    if LEAN_RESERVED.contains(&name) {
        format!("{name}'")
    } else if plain(name) && !LEAN_KEYWORDS.contains(&name) {
        name.to_string()
    } else {
        format!("«{name}»")
    }
}

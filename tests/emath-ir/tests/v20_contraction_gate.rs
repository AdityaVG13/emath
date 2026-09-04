use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ResidueKind {
    FeatureDispatch,
    StableIrVariant,
    ActiveRegistry,
    KernelAuthority,
    PublicSemanticModule,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Residue {
    kind: ResidueKind,
    file: String,
    line: usize,
    subject: String,
}

fn collect_files(root: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, extension, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}

fn quoted_literals(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            if quoted {
                value.push(character);
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            if quoted {
                values.push(std::mem::take(&mut value));
            }
            quoted = !quoted;
        } else if quoted {
            value.push(character);
        }
    }
    values
}

fn code_without_quoted_literals(line: &str) -> String {
    let mut code = String::with_capacity(line.len());
    let mut quoted = false;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            escaped = false;
        } else if quoted && character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if !quoted {
            code.push(character);
        }
    }
    code
}

fn authored_feature_names(root: &Path) -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_files(&root.join("language/spec"), "emath", &mut files);
    let mut names = BTreeSet::new();
    for file in files {
        let source = fs::read_to_string(file).unwrap();
        for line in source.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("feature_id:") {
                let id = value.trim().trim_matches('"');
                names.insert(id.to_string());
                if let Some(leaf) = id.rsplit('.').next() {
                    names.insert(leaf.to_string());
                }
            }
        }
    }
    names
}

fn selects_feature_identity(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "name",
        "feature",
        "capability",
        "callee",
        "operator",
        "operation",
    ]
    .iter()
    .any(|selector| {
        lower.contains(&format!("match {selector}"))
            || lower.starts_with(&format!("{selector} =="))
            || lower.starts_with(&format!("{selector} !="))
            || lower.contains(&format!(" {selector} =="))
            || lower.contains(&format!(" {selector} !="))
    })
}

fn is_branch(line: &str) -> bool {
    line.contains("=>")
        || line.contains("==")
        || line.contains("!=")
        || line.contains("starts_with(")
        || line.contains("strip_prefix(")
        || line.contains("match ")
}

fn is_feature_name(value: &str, names: &BTreeSet<String>) -> bool {
    names.contains(value)
        || value
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .any(|part| part.len() > 2 && names.contains(part))
}

fn is_universal_public_module(name: &str) -> bool {
    matches!(
        name,
        "diagnostic" | "hash" | "id" | "limits" | "parse" | "source" | "span" | "text" | "tree"
    )
}

fn module_is_semantic(name: &str, names: &BTreeSet<String>) -> bool {
    names.contains(name)
        || names.iter().any(|feature| {
            feature.len() > 3
                && name.len() > 3
                && (feature.contains(name) || name.contains(feature))
        })
}

// These instructions move opaque values, construct neutral carriers, or provide
// closed VM control. They do not identify a mathematical feature or decide a
// claim. Arithmetic over a carrier beyond this substrate belongs behind an
// authored FeatureID and ApplyCapability, even when its Rust kernel is reusable.
// `ProgramLiteral` is universal artifact machinery: a domain-neutral
// program-as-value carrier that names no FeatureID and dispatches nothing.
fn is_universal_ir_mechanism(variant: &str) -> bool {
    matches!(
        variant,
        "ProgramLiteral"
            | "ConstF64"
            | "ConstI64"
            | "ConstBigInt"
            | "ConstText"
            | "ConstComplex"
            | "ConstBool"
            | "FormatText"
            | "TextLength"
            | "TextNfc"
            | "ReportSection"
            | "ReportDocument"
            | "ReportMarkdown"
            | "ReportLatex"
            | "LoadInput"
            | "LoadState"
            | "F64Add"
            | "F64Sub"
            | "F64Mul"
            | "F64Div"
            | "F64Pow"
            | "Neg"
            | "UnaryBuiltin"
            | "BinaryBuiltin"
            | "Lt"
            | "Le"
            | "Gt"
            | "Ge"
            | "Eq"
            | "Ne"
            | "And"
            | "Or"
            | "Imply"
            | "Iff"
            | "Not"
            | "IsFinite"
            | "Select"
            | "SeriesCreate"
            | "SeriesSample"
            | "SetCreate"
            | "SetContains"
            | "RecordCreate"
            | "VectorCreate"
            | "MatrixCreate"
            | "TensorCreate"
            | "VectorIndex"
            | "MatrixIndex"
            | "TensorIndex"
            | "TensorSlice"
            | "OptionSome"
            | "OptionNone"
            | "OptionIsSome"
            | "OptionUnwrapOr"
            | "ResultOk"
            | "ResultErr"
            | "ResultIsOk"
            | "ResultUnwrapOr"
            | "ResultErrorOf"
            | "Fold"
            | "ApplyCapability"
            | "VectorMap"
            | "VectorMapScalar"
            | "VectorReduce"
            | "VectorAllFinite"
    )
}

fn emir_op_uses(line: &str) -> Vec<&str> {
    let mut variants = Vec::new();
    let mut rest = line;
    while let Some((_, after_prefix)) = rest.split_once("EmirOp::") {
        let variant = after_prefix
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .next()
            .unwrap_or("");
        if !variant.is_empty() {
            variants.push(variant);
        }
        rest = &after_prefix[variant.len()..];
    }
    variants
}

fn makes_mathematical_claim(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    is_branch(line)
        && (lower.contains("result_label")
            || lower.contains("claim_label")
            || lower.contains("claim =")
            || lower.contains("exactness =")
            || lower.contains("evidence =")
            || lower.contains("authority ="))
}

fn scan_source(
    relative: &str,
    source: &str,
    names: &BTreeSet<String>,
    residues: &mut Vec<Residue>,
) {
    let parser = relative.starts_with("crates/emath-syntax/src/");
    let sema = relative.starts_with("crates/emath-sema/src/");
    let stable_ir = relative.starts_with("crates/emath-ir/src/")
        || relative.starts_with("crates/emath-exec-ir/src/");
    let backend = relative.starts_with("crates/emath-rust-backend/src/");
    let runtime = relative.starts_with("crates/emath-rt/src/");
    let public_semantics = relative.starts_with("crates/emath-core/src/") || runtime;
    let registry = relative.contains("registry")
        || relative.contains("builtin")
        || relative.contains("catalog");
    let generated_data = relative.ends_with("language_image.rs")
        || relative.ends_with("language_tables.rs")
        || relative.ends_with("reference_views.rs");
    let kernel = backend
        || runtime
        || relative.starts_with("crates/emath-exec-ir/src/interp")
        || relative.starts_with("crates/emath-exec-ir/src/native_kernel")
        || relative.starts_with("crates/emath-exec-ir/src/optimize");
    let mut in_emir = false;
    let mut emir_depth = 0_i32;
    let mut identity_depth = 0_i32;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.starts_with("pub enum EmirOp") {
            in_emir = true;
        }
        if in_emir {
            emir_depth += line.matches('{').count() as i32;
            emir_depth -= line.matches('}').count() as i32;
            let variant = line
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next()
                .unwrap_or("");
            if !variant.is_empty()
                && variant.chars().next().is_some_and(char::is_uppercase)
                && variant != "EmirOp"
                && !is_universal_ir_mechanism(variant)
            {
                residues.push(Residue {
                    kind: ResidueKind::StableIrVariant,
                    file: relative.to_string(),
                    line: line_number,
                    subject: variant.to_string(),
                });
            }
            if emir_depth <= 0 && line.contains('}') {
                in_emir = false;
            }
        }
        let code = code_without_quoted_literals(line);
        for variant in emir_op_uses(&code) {
            if !is_universal_ir_mechanism(variant) {
                residues.push(Residue {
                    kind: ResidueKind::StableIrVariant,
                    file: relative.to_string(),
                    line: line_number,
                    subject: variant.to_string(),
                });
            }
        }

        let starts_identity_match = selects_feature_identity(line) && line.contains("match");
        let identity_context =
            starts_identity_match || identity_depth > 0 || selects_feature_identity(line);
        let literals = quoted_literals(line);
        if !generated_data
            && (parser || sema || stable_ir || backend || runtime)
            && is_branch(line)
            && literals.iter().any(|literal| {
                literal.starts_with("std.") || identity_context && is_feature_name(literal, names)
            })
        {
            for literal in &literals {
                if is_feature_name(literal, names) {
                    residues.push(Residue {
                        kind: ResidueKind::FeatureDispatch,
                        file: relative.to_string(),
                        line: line_number,
                        subject: literal.clone(),
                    });
                }
            }
        }
        if registry && !generated_data && (line.contains("insert(") || line.contains("from([")) {
            for literal in &literals {
                if is_feature_name(literal, names) {
                    residues.push(Residue {
                        kind: ResidueKind::ActiveRegistry,
                        file: relative.to_string(),
                        line: line_number,
                        subject: literal.clone(),
                    });
                }
            }
        }
        if kernel && makes_mathematical_claim(line) {
            residues.push(Residue {
                kind: ResidueKind::KernelAuthority,
                file: relative.to_string(),
                line: line_number,
                subject: line.to_string(),
            });
        }
        if public_semantics {
            if let Some(module) = line
                .strip_prefix("pub mod ")
                .and_then(|rest| rest.strip_suffix(';'))
            {
                if !is_universal_public_module(module.trim())
                    && module_is_semantic(module.trim(), names)
                {
                    residues.push(Residue {
                        kind: ResidueKind::PublicSemanticModule,
                        file: relative.to_string(),
                        line: line_number,
                        subject: module.trim().to_string(),
                    });
                }
            }
        }
        if starts_identity_match || identity_depth > 0 {
            identity_depth += line.matches('{').count() as i32;
            identity_depth -= line.matches('}').count() as i32;
            identity_depth = identity_depth.max(0);
        }
    }
}

// Honest active-source boundary: every compiled/declared Rust nucleus module
// and relevant test caller is scanned. No path class is exempt. Files that are
// not reachable from a crate root through `mod` declarations are retained
// unreferenced sources (pending deletion approval); their residue is reported
// honestly instead of being hidden, while residue reachable from referenced
// modules is a hard failure.
const SRC_DIRECTORIES: [&str; 7] = [
    "crates/emath-syntax/src",
    "crates/emath-sema/src",
    "crates/emath-ir/src",
    "crates/emath-exec-ir/src",
    "crates/emath-rust-backend/src",
    "crates/emath-rt/src",
    "crates/emath-core/src",
];

const TEST_CALLER_DIRECTORIES: [&str; 7] = [
    "tests/emath-syntax/tests",
    "tests/emath-sema/tests",
    "tests/emath-ir/tests",
    "tests/emath-exec-ir/tests",
    "tests/emath-rust-backend/tests",
    "tests/emath-rt/tests",
    "tests/emath-core/tests",
];

fn declared_module_paths(source: &str) -> Vec<String> {
    let mut modules = Vec::new();
    let mut explicit_path = None;
    for raw_line in source.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if let Some(path) = line
            .strip_prefix("#[path = \"")
            .and_then(|rest| rest.strip_suffix("\"]"))
        {
            explicit_path = Some(path.to_string());
            continue;
        }
        let declaration = line
            .strip_prefix("pub mod ")
            .or_else(|| line.strip_prefix("pub(crate) mod "))
            .or_else(|| line.strip_prefix("mod "))
            .and_then(|rest| rest.trim().strip_suffix(';'));
        if let Some(name) = declaration {
            if let Some(path) = explicit_path.take() {
                modules.push(path);
            } else {
                let name = name.trim();
                modules.push(format!("{name}.rs"));
                modules.push(format!("{name}/mod.rs"));
            }
        }
    }
    modules
}

fn src_file_relatives(root: &Path) -> BTreeSet<String> {
    let mut relatives = BTreeSet::new();
    for directory in SRC_DIRECTORIES {
        let mut files = Vec::new();
        collect_files(&root.join(directory), "rs", &mut files);
        for file in files {
            relatives.insert(
                file.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    relatives
}

fn module_search_directory(current: &str) -> String {
    let (parent, file) = current
        .rsplit_once('/')
        .map_or(("", current), |(parent, file)| (parent, file));
    match file {
        "lib.rs" | "main.rs" | "mod.rs" => format!("{parent}/"),
        _ => {
            let stem = file.strip_suffix(".rs").unwrap_or(file);
            format!("{parent}/{stem}/")
        }
    }
}

fn referenced_source_files(root: &Path, relatives: &BTreeSet<String>) -> BTreeSet<String> {
    let mut referenced = BTreeSet::new();
    let mut queue: Vec<String> = Vec::new();
    for directory in SRC_DIRECTORIES {
        for entry_point in ["lib.rs", "main.rs"] {
            let relative = format!("{directory}/{entry_point}");
            if relatives.contains(&relative) && referenced.insert(relative.clone()) {
                queue.push(relative);
            }
        }
    }
    while let Some(current) = queue.pop() {
        let source = match fs::read_to_string(root.join(&current)) {
            Ok(source) => source,
            Err(_) => continue,
        };
        let directory = module_search_directory(&current);
        for module_path in declared_module_paths(&source) {
            let candidate = format!("{directory}{module_path}");
            if relatives.contains(&candidate) && referenced.insert(candidate.clone()) {
                queue.push(candidate);
            }
        }
    }
    referenced
}

// Active gate inputs: every declared test caller is active by construction —
// cargo compiles each file under a crate's tests/ root as its own caller
// binary — alongside src files reachable through `mod` declarations. Without
// this, test-caller residue would be misclassified as retained and could
// never fail the gate.
fn is_active_gate_input(relative: &str, referenced: &BTreeSet<String>) -> bool {
    TEST_CALLER_DIRECTORIES
        .iter()
        .any(|directory| relative.starts_with(&format!("{directory}/")))
        || referenced.contains(relative)
}

fn partition_gate_residues(root: &Path, residues: Vec<Residue>) -> (Vec<Residue>, Vec<Residue>) {
    let referenced = referenced_source_files(root, &src_file_relatives(root));
    residues
        .into_iter()
        .partition(|residue| is_active_gate_input(&residue.file, &referenced))
}

fn scan_repository(root: &Path) -> Vec<Residue> {
    let names = authored_feature_names(root);
    let mut files = Vec::new();
    for directory in SRC_DIRECTORIES {
        collect_files(&root.join(directory), "rs", &mut files);
    }
    for directory in TEST_CALLER_DIRECTORIES {
        collect_files(&root.join(directory), "rs", &mut files);
    }
    files.sort();
    let mut residues = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        scan_source(
            &relative,
            &fs::read_to_string(&file).unwrap(),
            &names,
            &mut residues,
        );
    }
    residues.sort();
    residues.dedup();
    residues
}

#[test]
fn whole_nucleus_inventory_is_exact_and_actionable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let residues = scan_repository(&root);
    let (active, retained) = partition_gate_residues(&root, residues);
    // Honest ratchet: residue reachable from compiled/declared nucleus modules
    // or from any test-caller binary is forbidden and fails. Residue in
    // retained unreferenced files (pending deletion approval) is reported,
    // never forced to zero by an exemption.
    println!("retained unreferenced-file residue (pending deletion approval): {retained:#?}");
    assert!(
        active.is_empty(),
        "forbidden authority is reachable from compiled/declared nucleus modules or test callers; \
         retained unreferenced-file residue (pending deletion approval): {retained:#?}; \
         active residue: {active:#?}"
    );
}

#[test]
fn structural_gate_detects_every_forbidden_authority_shape() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let names = authored_feature_names(&root);
    let seeds = [
        (
            "crates/emath-syntax/src/parser.rs",
            "match name { \"std.capability.math.add\" => parse_add(), _ => generic() }",
            ResidueKind::FeatureDispatch,
        ),
        (
            "crates/emath-sema/src/admit.rs",
            "if operation == \"sum\" { admit_sum() }",
            ResidueKind::FeatureDispatch,
        ),
        (
            "crates/emath-exec-ir/src/op.rs",
            "pub enum EmirOp {\n    MatrixInverse,\n}",
            ResidueKind::StableIrVariant,
        ),
        (
            "crates/emath-rust-backend/src/emitter.rs",
            "match name { \"sum\" => emit_sum(), _ => emit_generic() }",
            ResidueKind::FeatureDispatch,
        ),
        (
            "crates/emath-rt/src/registry.rs",
            "map.insert(\"sum\", handwritten_sum);",
            ResidueKind::ActiveRegistry,
        ),
        (
            "crates/emath-rt/src/kernel.rs",
            "if world == \"exact\" { result_label = \"exact\"; }",
            ResidueKind::KernelAuthority,
        ),
        (
            "crates/emath-core/src/lib.rs",
            "pub mod probability;",
            ResidueKind::PublicSemanticModule,
        ),
    ];
    for (file, source, expected) in seeds {
        let mut residues = Vec::new();
        scan_source(file, source, &names, &mut residues);
        assert!(
            residues.iter().any(|residue| residue.kind == expected),
            "seed escaped: {file}: {source}"
        );
    }
}

#[test]
fn universal_ir_and_kernel_mechanisms_remain_legal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let names = authored_feature_names(&root);
    let sources = [
        (
            "crates/emath-exec-ir/src/op.rs",
            "pub enum EmirOp {\n    ConstF64(u64),\n    ApplyCapability { capability: String },\n}",
        ),
        (
            "crates/emath-exec-ir/src/kernel_binding.rs",
            "fn bind(feature: FeatureId, kernel: KernelId) -> KernelBinding { KernelBinding { feature, kernel } }",
        ),
        (
            "crates/emath-ir/src/image.rs",
            "if authority != \"structural-checked\" { refuse_unverified_metadata(); }",
        ),
    ];
    for (file, source) in sources {
        let mut residues = Vec::new();
        scan_source(file, source, &names, &mut residues);
        assert!(
            residues.is_empty(),
            "universal mechanism must remain legal: {file}: {residues:#?}"
        );
    }
}

// Failure-first proof for the honest active-source boundary: a forbidden
// module planted at a path the retired exemption used to blanket-skip must be
// caught by the repository scan. Against the old gate this assertion failed
// because `is_retired_source` skipped `crates/emath-core/src/geometry.rs`
// entirely, hiding reachable authority from the result.
#[test]
fn retired_exemption_cannot_hide_planted_forbidden_source() {
    let root = std::env::temp_dir().join(format!("emath-ehpal13-gate-{}", std::process::id()));
    let core = root.join("crates/emath-core/src/retired");
    fs::create_dir_all(&core).unwrap();
    fs::write(
        root.join("crates/emath-core/src/lib.rs"),
        "#[path = \"retired/geometry.rs\"]\nmod geometry;\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/emath-core/src/retired/geometry.rs"),
        "pub enum EmirOp {\n    MatrixInverse,\n}\n",
    )
    .unwrap();
    let (active, retained) = partition_gate_residues(&root, scan_repository(&root));
    assert!(
        active
            .iter()
            .any(|residue| residue.kind == ResidueKind::StableIrVariant
                && residue.file == "crates/emath-core/src/retired/geometry.rs"),
        "path-attributed forbidden source must be active; active: {active:#?} retained: {retained:#?}"
    );
}

// Rust resolves `mod child;` in `parent.rs` from `parent/child.rs`, not beside
// `parent.rs`. The active boundary must follow that rule while leaving
// undeclared source retained.
#[test]
fn nested_module_reachability_distinguishes_compiled_and_uncompiled_sources() {
    let root = std::env::temp_dir().join(format!("emath-ehpal13-nested-{}", std::process::id()));
    let src = root.join("crates/emath-exec-ir/src");
    fs::create_dir_all(src.join("term_compile")).unwrap();
    fs::create_dir_all(src.join("emitter/call")).unwrap();
    fs::write(
        src.join("lib.rs"),
        "pub mod term_compile;\npub mod emitter;\n",
    )
    .unwrap();
    fs::write(
        src.join("term_compile.rs"),
        "mod registry;\npub const LEGAL: u8 = 1;\n",
    )
    .unwrap();
    fs::write(
        src.join("term_compile/registry.rs"),
        "pub enum EmirOp {\n    MatrixInverse,\n}\n",
    )
    .unwrap();
    fs::write(src.join("emitter.rs"), "pub const LEGAL: u8 = 1;\n").unwrap();
    fs::write(
        src.join("emitter/call/math_misc.rs"),
        "pub enum EmirOp {\n    MatrixInverse,\n}\n",
    )
    .unwrap();

    let (active, retained) = partition_gate_residues(&root, scan_repository(&root));
    assert!(
        active
            .iter()
            .any(|residue| residue.file
                == "crates/emath-exec-ir/src/term_compile/registry.rs"),
        "declared nested module must be active; active: {active:#?} retained: {retained:#?}"
    );
    assert!(
        retained
            .iter()
            .any(|residue| residue.file
                == "crates/emath-exec-ir/src/emitter/call/math_misc.rs"),
        "undeclared nested source must remain retained; active: {active:#?} retained: {retained:#?}"
    );
}

// Ordinary domain vocabulary in data or prose is not authority. Without a
// branch on a feature identity there is no dispatch, registry entry, or claim.
#[test]
fn ordinary_domain_words_in_data_do_not_flag() {
    let root = std::env::temp_dir().join(format!("emath-ehpal13-fp-{}", std::process::id()));
    let core = root.join("crates/emath-core/src");
    fs::create_dir_all(&core).unwrap();
    fs::write(
        root.join("crates/emath-core/src/lib.rs"),
        "pub const DOMAIN_NOTES: &str = \"probability theory and geometry vocabulary in prose or data\";\n",
    )
    .unwrap();
    let residues = scan_repository(&root);
    assert!(
        residues.is_empty(),
        "ordinary domain words in data must not be flagged: {residues:#?}"
    );
}

// Mutation proof for active test-caller classification: an obsolete op in a
// test-caller binary must fail as active, not hide as retained. Against the
// referenced-only partition this assertion fails because test files were
// never reachable from a crate root and were therefore misclassified.
#[test]
fn test_caller_residue_fails_active_classification() {
    let root = std::env::temp_dir().join(format!("emath-ehpal13-caller-{}", std::process::id()));
    let core = root.join("crates/emath-core/src");
    let callers = root.join("tests/emath-core/tests");
    fs::create_dir_all(&core).unwrap();
    fs::create_dir_all(&callers).unwrap();
    fs::write(
        root.join("crates/emath-core/src/lib.rs"),
        "pub const LEGAL: u8 = 1;\n",
    )
    .unwrap();
    fs::write(
        root.join("tests/emath-core/tests/caller.rs"),
        "fn obsolete_caller() {\n    let _ = EmirOp::MatrixInverse;\n}\n",
    )
    .unwrap();
    let (active, retained) = partition_gate_residues(&root, scan_repository(&root));
    assert!(
        active
            .iter()
            .any(|residue| residue.kind == ResidueKind::StableIrVariant
                && residue.file == "tests/emath-core/tests/caller.rs"),
        "test-caller residue must fail active classification; active: {active:#?} retained: {retained:#?}"
    );
}

#[test]
fn generated_and_authored_authority_remain_separate() {
    let tables = fs::read_to_string("../../crates/emath-exec-ir/src/language_tables.rs").unwrap();
    assert!(tables.contains("DO NOT EDIT"));
    let authored = fs::read_to_string("../../language/spec/capabilities/core/add.emath").unwrap();
    assert!(!authored.contains("@generated"));
    assert!(authored.contains("std.capability.math.add"));
}

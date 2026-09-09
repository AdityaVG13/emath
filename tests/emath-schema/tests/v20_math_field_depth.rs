//! V20 MATH bucket B at field depth: characteristic operations in spec/,
//! not one-identity witnesses and not leftover `x = N`.
//!
//! `examples/` is a teaching set. This probe reads authored capsules and a
//! tests/ fixture. Failure-first: it must fail until the field's operations
//! are installed.

use std::collections::HashMap;

use emath_test_harness::{Probe, workspace_file, workspace_path};

fn leftover_output_assigns(source: &str) -> Vec<(String, String)> {
    let mut leftover = Vec::new();
    let mut outputs = Vec::new();
    let mut in_outputs = false;
    let mut in_defs = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed == "outputs:" {
            in_outputs = true;
            in_defs = false;
            outputs.clear();
            continue;
        }
        if trimmed == "definitions:" {
            in_outputs = false;
            in_defs = true;
            continue;
        }
        if trimmed == "inputs:"
            || trimmed == "tests:"
            || trimmed == "goals:"
            || trimmed.starts_with("emath ")
        {
            in_outputs = false;
            in_defs = false;
            continue;
        }
        if in_outputs && line.starts_with("        ") {
            if let Some(name) = trimmed.split(':').next() {
                outputs.push(name.trim().to_string());
            }
        }
        if in_defs {
            let Some((name, rhs)) = split_assign(trimmed) else {
                continue;
            };
            if outputs.iter().any(|output| output == name) && is_bare_literal(rhs) {
                leftover.push((name.to_string(), rhs.to_string()));
            }
        }
    }
    leftover
}

fn split_assign(line: &str) -> Option<(&str, &str)> {
    let (name, rhs) = line.split_once(" = ")?;
    if name.contains(' ') {
        return None;
    }
    Some((name, rhs.trim()))
}

fn is_bare_literal(rhs: &str) -> bool {
    matches!(rhs, "true" | "false")
        || rhs.parse::<i128>().is_ok()
        || rhs.parse::<f64>().is_ok() && !rhs.contains(['+', '*', '/', '-'].as_ref()) && rhs != "-0"
            && !rhs.contains("rat")
        || bare_rat(rhs)
}

fn bare_rat(rhs: &str) -> bool {
    let Some(inner) = rhs.strip_prefix("rat(").and_then(|rest| rest.strip_suffix(')')) else {
        return false;
    };
    let mut parts = inner.split(',');
    let (Some(num), Some(den), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    num.trim().parse::<i128>().is_ok() && den.trim().parse::<i128>().is_ok()
}

fn spec_feature_blocks() -> HashMap<String, String> {
    let mut blocks = HashMap::new();
    let root = workspace_path("language/spec/capabilities");
    let walker = walkdir_emath(&root);
    let mut current_id = None;
    let mut buf = String::new();
    for path in walker {
        let source = std::fs::read_to_string(&path).expect("read capsule");
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("emath feature ") {
                if let Some(id) = current_id.take() {
                    blocks.insert(id, buf);
                    buf = String::new();
                }
            }
            buf.push_str(line);
            buf.push('\n');
            if let Some(rest) = trimmed.strip_prefix("feature_id:") {
                let id = rest.trim().trim_matches('"').to_string();
                current_id = Some(id);
            }
        }
        if let Some(id) = current_id.take() {
            blocks.insert(id, buf);
            buf = String::new();
        }
    }
    blocks
}

fn walkdir_emath(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    fn rec(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rec(&path, files);
            } else if path.extension().and_then(|e| e.to_str()) == Some("emath") {
                files.push(path);
            }
        }
    }
    rec(root, &mut files);
    files.sort();
    files
}

fn formula_computes(block: &str) -> bool {
    let Some(sem) = block.lines().find(|l| l.trim().starts_with("semantics:")) else {
        return false;
    };
    sem.contains("compose=")
        && sem.contains("formula=")
        && !sem.contains("formula=6;")
        && !sem.contains("formula=true;")
        && (sem.contains('*') || sem.contains('+') || sem.contains('/') || sem.contains("rem"))
}

#[test]
fn finite_group_theory_field_depth() {
    let mut probe = Probe::new(
        "finite group theory: order, inverse, Lagrange index compute (not leftover |S3|=6)",
    );
    let blocks = spec_feature_blocks();
    let required = [
        (
            "std.capability.algebra.group-order-six",
            "3*2*1",
            "order |S3|=3!",
        ),
        (
            "std.capability.group.lagrange-s3",
            "3!/(3!/2)",
            "Lagrange index |S3|/|A3|",
        ),
        (
            "std.capability.algebra.finite-group-inverse",
            "2*3 rem 5",
            "inverse in (Z/5Z)*",
        ),
        (
            "std.capability.algebra.finite-group-identity",
            "1*g",
            "left identity",
        ),
        (
            "std.capability.algebra.finite-group-lagrange-divides",
            "6/2",
            "Lagrange: |H| divides |G|",
        ),
        (
            "std.capability.algebra.finite-group-element-order",
            "2^4 rem 5",
            "element order in (Z/5Z)*",
        ),
    ];
    for (id, formula_needle, label) in required {
        match blocks.get(id) {
            Some(block) => {
                probe.demand(
                    format!("{id}/computes"),
                    formula_computes(block),
                    format!("{label} capsule must compose a formula, not leftover x=N"),
                );
                probe.contains(format!("{id}/formula"), block, formula_needle);
            }
            None => {
                probe.fail(format!("{id}/present"), format!("missing {label} FeatureID {id}"));
            }
        }
    }

    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-field-depth-heading",
        &capability,
        "## Field-depth MATH",
    );
    probe.contains(
        "capability-finite-groups-ops",
        &capability,
        "algebra-and-number-theory-finite-group-theory",
    );
    probe.contains("capability-mentions-inverse", &capability, "inverse");
    probe.contains("capability-mentions-lagrange", &capability, "Lagrange");

    let fixture_rel = "tests/fixtures/v20/finite-group-ops.emath";
    let fixture_path = workspace_path(fixture_rel);
    probe.demand(
        "fixture-exists",
        fixture_path.is_file(),
        format!("{fixture_rel} must exist as a user-shaped compute program"),
    );
    if fixture_path.is_file() {
        let source = workspace_file(fixture_rel);
        for (output, rhs) in leftover_output_assigns(&source) {
            probe.fail(
                format!("leftover-{output}"),
                format!("{fixture_rel} leftover {output} = {rhs}"),
            );
        }
        probe.contains("fixture-order", &source, "3 * 2 * 1");
        probe.contains("fixture-inverse", &source, "int_rem");
        probe.contains("fixture-lagrange", &source, "factorial(3)");
        probe.contains("fixture-identity", &source, "1 * g");
    }

    probe.finish();
}

fn require_ids(
    probe: &mut Probe,
    blocks: &HashMap<String, String>,
    required: &[(&str, &str, &str)],
    must_formula_compute: bool,
) {
    for (id, formula_needle, label) in required {
        match blocks.get(*id) {
            Some(block) => {
                probe.demand(
                    format!("{id}/compose"),
                    block.contains("compose="),
                    format!("{label} must compose existing FeatureIDs"),
                );
                if must_formula_compute {
                    probe.demand(
                        format!("{id}/computes"),
                        formula_computes(block),
                        format!("{label} capsule must compose a formula, not leftover x=N"),
                    );
                }
                probe.contains(format!("{id}/formula"), block, formula_needle);
            }
            None => {
                probe.fail(
                    format!("{id}/present"),
                    format!("missing {label} FeatureID {id}"),
                );
            }
        }
    }
}

fn require_fixture(probe: &mut Probe, rel: &str, needles: &[&str]) {
    let fixture_path = workspace_path(rel);
    probe.demand(
        format!("{rel}/exists"),
        fixture_path.is_file(),
        format!("{rel} must exist as a user-shaped compute program"),
    );
    if fixture_path.is_file() {
        let source = workspace_file(rel);
        for (output, rhs) in leftover_output_assigns(&source) {
            probe.fail(
                format!("leftover-{rel}-{output}"),
                format!("{rel} leftover {output} = {rhs}"),
            );
        }
        for needle in needles {
            probe.contains(format!("{rel}/{needle}"), &source, needle);
        }
    }
}

#[test]
fn elementary_algebra_field_depth() {
    let mut probe = Probe::new(
        "elementary algebra: binomial square/cube, difference of squares, Vieta compute",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.binomial-square",
                "binomial-square",
                "(a+b)^2",
            ),
            (
                "std.capability.algebra.binomial-cube",
                "binomial-cube",
                "(a+b)^3",
            ),
            (
                "std.capability.algebra.diff-squares",
                "diff-squares",
                "a^2-b^2",
            ),
            ("std.capability.algebra.vieta", "vieta", "Vieta sum/product"),
        ],
        false,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-elem-alg",
        &capability,
        "algebra-and-number-theory-elementary-algebra",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/elementary-algebra-ops.emath",
        &[
            "(a + b) * (a + b)",
            "(a + b) * (a + b) * (a + b)",
            "a * a - b * b",
            "r + s",
            "r * s",
        ],
    );
    probe.finish();
}

#[test]
fn monoid_theory_field_depth() {
    let mut probe = Probe::new("monoid theory: unit and associativity compute, not only 1*1=1");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.monoid-one",
                "1*1",
                "multiplicative unit on 1",
            ),
            (
                "std.capability.algebra.monoid-left-unit",
                "1*x",
                "left unit",
            ),
            (
                "std.capability.algebra.monoid-right-unit",
                "x*1",
                "right unit",
            ),
            (
                "std.capability.algebra.magma-assoc",
                "(2*3)*4",
                "associativity",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-monoid",
        &capability,
        "algebra-and-number-theory-monoid-theory",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/monoid-ops.emath",
        &["1 * x", "x * 1", "(2 * 3) * 4", "2 * (3 * 4)"],
    );
    probe.finish();
}

#[test]
fn semigroup_theory_field_depth() {
    let mut probe = Probe::new("semigroup theory: associativity, order, idempotent compute");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.semigroup-six",
                "3*2*1",
                "finite semigroup order slice",
            ),
            (
                "std.capability.algebra.magma-assoc",
                "(2*3)*4",
                "associativity",
            ),
            (
                "std.capability.algebra.semigroup-idempotent",
                "0*0",
                "idempotent 0*0=0",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-semigroup",
        &capability,
        "algebra-and-number-theory-semigroup-theory",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/semigroup-ops.emath",
        &["3 * 2 * 1", "(2 * 3) * 4", "0 * 0"],
    );
    probe.finish();
}

#[test]
fn commutative_algebra_field_depth() {
    let mut probe = Probe::new("commutative algebra: ab=ba and a+b=b+a compute");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.commute-five",
                "2*3",
                "product slice",
            ),
            (
                "std.capability.algebra.commute-mul-law",
                "(2*3)==(3*2)",
                "multiplicative commutativity",
            ),
            (
                "std.capability.algebra.commute-add-law",
                "(2+3)==(3+2)",
                "additive commutativity",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-commute",
        &capability,
        "algebra-and-number-theory-commutative-algebra",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/commutative-algebra-ops.emath",
        &["2 * 3 == 3 * 2", "2 + 3 == 3 + 2"],
    );
    probe.finish();
}

#[test]
fn ring_theory_field_depth() {
    let mut probe = Probe::new("ring theory: distribute and units compute");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.ring-distribute",
                "2*(3+4)",
                "distributivity",
            ),
            (
                "std.capability.algebra.semiring-zero-add",
                "0+5",
                "additive unit",
            ),
            (
                "std.capability.algebra.monoid-left-unit",
                "1*x",
                "multiplicative unit",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-ring",
        &capability,
        "algebra-and-number-theory-ring-theory",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/ring-ops.emath",
        &["2 * (3 + 4)", "0 + 5", "1 * 5"],
    );
    probe.finish();
}

#[test]
fn tropical_algebra_field_depth() {
    let mut probe = Probe::new("tropical algebra: min-plus times and unit compute");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            ("std.capability.algebra.tropical-meet", "min", "tropical add/min"),
            (
                "std.capability.algebra.tropical-times",
                "2+5",
                "tropical multiply",
            ),
            (
                "std.capability.algebra.tropical-mul-unit",
                "2+0",
                "tropical multiplicative unit",
            ),
        ],
        false,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-tropical",
        &capability,
        "algebra-and-number-theory-tropical-algebra",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/tropical-ops.emath",
        &["min(2, 3)", "2 + 5", "2 + 0"],
    );
    probe.finish();
}

#[test]
fn quaternion_algebra_field_depth() {
    let mut probe = Probe::new("quaternion algebra: norm-squared and i^2=-1 compute");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.quat-normsq",
                "1*1+2*2+2*2+4*4",
                "norm squared",
            ),
            (
                "std.capability.algebra.quat-i-square",
                "1+(-1)",
                "i^2 = -1",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-quat",
        &capability,
        "algebra-and-number-theory-quaternion-algebra",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/quaternion-ops.emath",
        &["1 * 1 + 2 * 2 + 2 * 2 + 4 * 4", "1 + (-1)"],
    );
    probe.finish();
}

#[test]
fn field_and_galois_field_depth() {
    let mut probe = Probe::new("field/Galois: characteristic, inverse, tower compute");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.field-char-five",
                "1+1+1+1+1",
                "char F_5",
            ),
            (
                "std.capability.algebra.finite-group-inverse",
                "2*3 rem 5",
                "field inverse",
            ),
            ("std.capability.algebra.galois-quad", "4/2", "tower law"),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-field",
        &capability,
        "algebra-and-number-theory-field-theory",
    );
    probe.contains(
        "capability-galois",
        &capability,
        "algebra-and-number-theory-galois-theory",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/field-module-clifford-ops.emath",
        &["4 / 2", "1 + 1 + 1 + 1 + 1", "int_rem"],
    );
    probe.finish();
}

#[test]
fn module_and_clifford_field_depth() {
    let mut probe = Probe::new("module/Clifford: rank sum/tensor and Cl(4) dim compute");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.module-rank",
                "3*4",
                "tensor rank",
            ),
            (
                "std.capability.algebra.module-rank-add",
                "3+4",
                "direct-sum rank",
            ),
            (
                "std.capability.algebra.clifford-eight",
                "2*2*2*2",
                "Cl(4) dimension",
            ),
            (
                "std.capability.algebra.clifford-square-one",
                "1*1",
                "e_i^2=1",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    probe.contains(
        "capability-module",
        &capability,
        "algebra-and-number-theory-module-theory",
    );
    probe.contains(
        "capability-clifford",
        &capability,
        "algebra-and-number-theory-clifford-algebra",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/field-module-clifford-ops.emath",
        &["3 * 4", "3 + 4", "2 * 2 * 2 * 2", "1 * 1"],
    );
    probe.finish();
}

fn algebra_more_capability(probe: &mut Probe, capability: &str, catalog: &str) {
    probe.contains(format!("capability-{catalog}"), capability, catalog);
}

#[test]
fn lie_and_representation_field_depth() {
    let mut probe = Probe::new("Lie groups and representation theory field-depth operations");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.lie-sl2",
                "2*2-1",
                "dim sl(2)",
            ),
            (
                "std.capability.algebra.lie-sl3-dim",
                "3*3-1",
                "dim sl(3)",
            ),
            (
                "std.capability.algebra.lie-bracket-skew",
                "1+(-1)",
                "Lie bracket skew",
            ),
            (
                "std.capability.algebra.rep-dim",
                "3*2*1",
                "regular representation dim",
            ),
            (
                "std.capability.algebra.rep-trivial-dim",
                "1+0",
                "trivial representation dim",
            ),
            (
                "std.capability.algebra.rep-hom-dim",
                "2*3",
                "dim Hom",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-lie-groups",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-representation-theory",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/algebra-more-ops.emath",
        &["3 * 3 - 1", "1 + (-1)", "1 + 0", "2 * 3"],
    );
    probe.finish();
}

#[test]
fn homology_elliptic_padic_field_depth() {
    let mut probe = Probe::new("homology, elliptic curves, p-adic field-depth operations");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.homology-cycle",
                "1-0",
                "cycle minus boundary",
            ),
        ],
        false,
    );
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.homology-euler",
                "1+(-0)",
                "Euler characteristic of a point",
            ),
            (
                "std.capability.algebra.elliptic-disc",
                "-16*27",
                "Weierstrass discriminant",
            ),
            (
                "std.capability.algebra.elliptic-j-zero",
                "0*1",
                "j-invariant 0",
            ),
            (
                "std.capability.algebra.padic-val",
                "3+5",
                "additive valuation",
            ),
            (
                "std.capability.algebra.padic-val-unit",
                "0*1",
                "valuation of a unit",
            ),
            (
                "std.capability.algebra.padic-val-p",
                "1+0",
                "v_p(p)",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-homological-algebra",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-elliptic-curves",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-p-adic-mathematics",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/algebra-more-ops.emath",
        &["-16 * 27", "0 * 1", "int_rem(8, 3)"],
    );
    probe.finish();
}

#[test]
fn tensor_groebner_universal_field_depth() {
    let mut probe = Probe::new("tensor, Gröbner, universal algebra field-depth operations");
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.tensor-rank-two",
                "3*4",
                "Kronecker rank",
            ),
            (
                "std.capability.algebra.tensor-rank-assoc",
                "(2*3)*4",
                "associative tensor rank",
            ),
            (
                "std.capability.algebra.groebner-deg",
                "5+8",
                "monomial degree add",
            ),
            (
                "std.capability.algebra.grobner-lead-two",
                "6+8",
                "leading total degree",
            ),
            (
                "std.capability.algebra.groebner-rem-zero",
                "6+(-6)",
                "remainder of a GB member",
            ),
            (
                "std.capability.algebra.univ-arity-two",
                "1+3+4",
                "binary term size",
            ),
            (
                "std.capability.algebra.univ-compose-arity",
                "2+1",
                "compose arity",
            ),
            (
                "std.capability.algebra.general-alg-ops-two",
                "2*(3+4)",
                "distributivity slice",
            ),
            (
                "std.capability.algebra.assoc-twenty-four",
                "2*3*4",
                "associative product",
            ),
            (
                "std.capability.algebra.exterior-dim-three",
                "2*2*2",
                "exterior algebra dim",
            ),
            (
                "std.capability.algebra.octonion-ones",
                "1+1+1+1+1+1+1+1",
                "octonion unit-norm pieces",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-tensor-algebra",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-gr-bner-basis-theory",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-universal-algebra",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-associative-algebra",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-exterior-algebra",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-octonion-algebra",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/algebra-more-ops.emath",
        &["(2 * 3) * 4", "6 + (-6)", "2 + 1", "2 * 2 * 2"],
    );
    probe.finish();
}

#[test]
fn category_geometry_nt_field_depth() {
    let mut probe = Probe::new(
        "category, geometric groups, analytic/modular/Diophantine/computational NT",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.two-morph-dim",
                "((1+2)+(3+4))-((1+3)+(2+4))",
                "Eckmann-Hilton",
            ),
            (
                "std.capability.algebra.cat-horiz-id",
                "(1+2)-2",
                "identity composition",
            ),
            (
                "std.capability.algebra.cayley-degree-two",
                "2*3",
                "Cayley degree",
            ),
            (
                "std.capability.algebra.cayley-ball",
                "1+2*3",
                "Cayley 1-ball",
            ),
        ],
        true,
    );
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.analytic-pi-two",
                "5-3",
                "twin-prime gap",
            ),
        ],
        false,
    );
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.algebra.analytic-pi-five",
                "2+1",
                "pi(5)",
            ),
            (
                "std.capability.algebra.modular-weight",
                "3*4",
                "weight of E4^3",
            ),
            (
                "std.capability.algebra.modular-dim-twelve",
                "1+1",
                "dim M_12",
            ),
            (
                "std.capability.algebra.automorphic-level-one",
                "8*3/4",
                "index Gamma(2)",
            ),
            (
                "std.capability.algebra.dioph-q-two",
                "4+7",
                "Farey denominator",
            ),
            (
                "std.capability.algebra.dioph-mediant-num",
                "1+1",
                "Farey mediant numerator",
            ),
            (
                "std.capability.algebra.dioph-farey-width",
                "1*7-1*4",
                "Farey neighbor determinant",
            ),
            (
                "std.capability.algebra.crt-witness-rem",
                "8 rem 3",
                "CRT remainder",
            ),
            (
                "std.capability.algebra.trial-div-five",
                "15/3",
                "trial division",
            ),
            (
                "std.capability.algebra.totient-seven",
                "7+(-1)",
                "Euler totient of 7",
            ),
            (
                "std.capability.algebra.alg-nt-disc",
                "1+(-4)",
                "quadratic discriminant",
            ),
            (
                "std.capability.algebra.arith-genus-zero",
                "(1-1)*(1-2)/2",
                "arithmetic genus of a line",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-category-theory",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-geometric-group-theory",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-analytic-number-theory",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-modular-forms",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-computational-number-theory",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-algebraic-number-theory",
    );
    algebra_more_capability(
        &mut probe,
        &capability,
        "algebra-and-number-theory-k-theory",
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/algebra-more-ops.emath",
        &[
            "1 + 2 * 3",
            "int_rem(8, 3)",
            "7 + (-1)",
            "1 + (-4)",
            "(1 + 2) - 2",
        ],
    );
    probe.finish();
}

#[test]
fn analysis_fields_depth() {
    let mut probe = Probe::new(
        "analysis MATH: characteristic calculus/series/ODE/PDE operations compute",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.analysis.calc-sum-rule",
                "2+3",
                "calculus sum rule",
            ),
            (
                "std.capability.analysis.calc-power-three",
                "3*4",
                "power rule",
            ),
            (
                "std.capability.numerics.autodiff-chain-six",
                "4*8",
                "chain rule product",
            ),
            (
                "std.capability.analysis.complex-abs-sq",
                "1*1+2*2",
                "complex modulus squared",
            ),
            (
                "std.capability.analysis.complex-prod-re",
                "1*3-2*4",
                "complex product real part",
            ),
            (
                "std.capability.analysis.fourier-dc-sum",
                "1+2+3+4",
                "Fourier DC",
            ),
            (
                "std.capability.analysis.parseval-five",
                "4*4+5*5",
                "Parseval energy",
            ),
            (
                "std.capability.analysis.geometric-series",
                "1+2+4+8",
                "geometric series",
            ),
            (
                "std.capability.analysis.partial-sums",
                "1+2+3+4",
                "arithmetic partial sum",
            ),
            (
                "std.capability.analysis.rec-step",
                "2*3+1",
                "difference equation step",
            ),
            (
                "std.capability.analysis.euler-step",
                "1+1*2",
                "ODE Euler step",
            ),
            (
                "std.capability.analysis.heat-explicit",
                "15+(-4)",
                "heat update",
            ),
            (
                "std.capability.analysis.cons-residual",
                "11+(-4)+(-7)",
                "conservation residual",
            ),
            (
                "std.capability.analysis.riemann-rect",
                "2*3",
                "Riemann rectangle",
            ),
            (
                "std.capability.analysis.trap-eval",
                "(1+3)*2/2",
                "trapezoid",
            ),
            (
                "std.capability.analysis.jensen-mid",
                "(2+8)/2",
                "Jensen midpoint",
            ),
            (
                "std.capability.analysis.gamma-five",
                "4*3*2*1",
                "Gamma(5)",
            ),
            (
                "std.capability.analysis.choose-five-two",
                "5*4/2",
                "binomial C(5,2)",
            ),
            (
                "std.capability.analysis.frac-half-add",
                "2*1/2",
                "fractional half-steps",
            ),
            (
                "std.capability.analysis.lip-bound",
                "(8-2)/(4-1)",
                "Lipschitz bound",
            ),
            (
                "std.capability.analysis.ham-energy",
                "3+5",
                "Hamiltonian T+V",
            ),
            (
                "std.capability.analysis.lag-diff",
                "5+(-2)",
                "Lagrangian T-V",
            ),
            (
                "std.capability.analysis.dae-constraint",
                "1+(-1)",
                "DAE constraint",
            ),
            (
                "std.capability.analysis.sde-drift",
                "2+3*1",
                "SDE drift",
            ),
            (
                "std.capability.analysis.arith-closed",
                "5*(4+12)/2",
                "arithmetic closed form",
            ),
            (
                "std.capability.analysis.react-diff-bal",
                "2+3+(-5)",
                "reaction-diffusion balance",
            ),
            (
                "std.capability.analysis.wave-cfl",
                "2*1/2",
                "CFL number",
            ),
            (
                "std.capability.analysis.mixed-partial",
                "(5*4)-(3*2)",
                "mixed partials",
            ),
            (
                "std.capability.analysis.curl-comp",
                "2+(-2)",
                "curl component",
            ),
            (
                "std.capability.analysis.cauchy-add",
                "2+3",
                "Cauchy equation",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    for catalog in [
        "analysis-calculus-and-equations-single-variable-calculus",
        "analysis-calculus-and-equations-multivariable-calculus",
        "analysis-calculus-and-equations-vector-calculus",
        "analysis-calculus-and-equations-tensor-calculus",
        "analysis-calculus-and-equations-real-analysis",
        "analysis-calculus-and-equations-complex-analysis",
        "analysis-calculus-and-equations-several-complex-variables",
        "analysis-calculus-and-equations-measure-theory",
        "analysis-calculus-and-equations-integration-theory",
        "analysis-calculus-and-equations-functional-analysis",
        "analysis-calculus-and-equations-operator-theory",
        "analysis-calculus-and-equations-operator-algebras",
        "analysis-calculus-and-equations-spectral-theory",
        "analysis-calculus-and-equations-harmonic-analysis",
        "analysis-calculus-and-equations-abstract-harmonic-analysis",
        "analysis-calculus-and-equations-fourier-analysis",
        "analysis-calculus-and-equations-wavelet-analysis",
        "analysis-calculus-and-equations-potential-theory",
        "analysis-calculus-and-equations-special-functions",
        "analysis-calculus-and-equations-asymptotic-analysis",
        "analysis-calculus-and-equations-approximation-theory",
        "analysis-calculus-and-equations-sequences-and-series",
        "analysis-calculus-and-equations-integral-transforms",
        "analysis-calculus-and-equations-integral-equations",
        "analysis-calculus-and-equations-functional-equations",
        "analysis-calculus-and-equations-difference-equations",
        "analysis-calculus-and-equations-fractional-calculus",
        "analysis-calculus-and-equations-distribution-theory",
        "analysis-calculus-and-equations-sobolev-spaces",
        "analysis-calculus-and-equations-microlocal-analysis",
        "analysis-calculus-and-equations-nonlinear-analysis",
        "analysis-calculus-and-equations-fixed-point-theory",
        "analysis-calculus-and-equations-convex-analysis",
        "analysis-calculus-and-equations-calculus-of-variations",
        "analysis-calculus-and-equations-optimal-transport",
        "analysis-calculus-and-equations-inverse-problems",
        "analysis-calculus-and-equations-geometric-analysis",
        "analysis-calculus-and-equations-ordinary-differential-equations",
        "analysis-calculus-and-equations-partial-differential-equations",
        "analysis-calculus-and-equations-differential-algebraic-equations",
        "analysis-calculus-and-equations-delay-differential-equations",
        "analysis-calculus-and-equations-stochastic-differential-equations",
        "analysis-calculus-and-equations-elliptic-pde",
        "analysis-calculus-and-equations-parabolic-pde",
        "analysis-calculus-and-equations-hyperbolic-pde",
        "analysis-calculus-and-equations-conservation-laws",
        "analysis-calculus-and-equations-reaction-diffusion-systems",
        "analysis-calculus-and-equations-boundary-value-problems",
        "analysis-calculus-and-equations-initial-value-problems",
        "analysis-calculus-and-equations-dynamical-systems",
        "analysis-calculus-and-equations-ergodic-theory",
        "analysis-calculus-and-equations-chaos-theory",
        "analysis-calculus-and-equations-bifurcation-theory",
        "analysis-calculus-and-equations-hamiltonian-systems",
        "analysis-calculus-and-equations-lagrangian-systems",
        "analysis-calculus-and-equations-arithmetic-and-pre-calculus",
    ] {
        algebra_more_capability(&mut probe, &capability, catalog);
    }
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/analysis-ops.emath",
        &[
            "2 + 3",
            "3 * 4",
            "1 * 1 + 2 * 2",
            "1 * 3 - 2 * 4",
            "2 * 3 + 1",
            "15 + (-4)",
            "11 + (-4) + (-7)",
            "4 * 3 * 2 * 1",
            "5 * (4 + 12) / 2",
        ],
    );
    probe.finish();
}

#[test]
fn discrete_fields_depth() {
    let mut probe = Probe::new(
        "discrete combinatorics MATH: graphs, designs, partitions, words compute",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.discrete.disc-choose-52",
                "5*4/2",
                "C(5,2)",
            ),
            (
                "std.capability.discrete.disc-tri-five",
                "5*6/2",
                "triangular T_5",
            ),
            (
                "std.capability.discrete.disc-catalan-three",
                "6*5*4/(4*3*2)",
                "Catalan C_3",
            ),
            (
                "std.capability.discrete.disc-stirling-42",
                "6+1",
                "Stirling S(4,2)",
            ),
            (
                "std.capability.discrete.disc-ramsey-six",
                "2*3",
                "Ramsey R(3,3)",
            ),
            (
                "std.capability.discrete.disc-sumset-min",
                "2*3+(-1)",
                "sumset doubling",
            ),
            (
                "std.capability.discrete.disc-k5-edges",
                "5*4/2",
                "complete graph edges",
            ),
            (
                "std.capability.discrete.disc-tree-seven",
                "7+(-1)",
                "tree edges",
            ),
            (
                "std.capability.discrete.disc-handshake",
                "2*6",
                "handshaking lemma",
            ),
            (
                "std.capability.discrete.disc-hyper-c43",
                "4*3*2/(3*2*1)",
                "hypergraph triples",
            ),
            (
                "std.capability.discrete.disc-planar-bound",
                "3*5+(-6)",
                "planar 3v-6",
            ),
            (
                "std.capability.discrete.disc-latin-cells",
                "5*5",
                "latin square cells",
            ),
            (
                "std.capability.discrete.disc-matroid-sub",
                "3+4+(-5)+(-2)",
                "matroid submodularity",
            ),
            (
                "std.capability.discrete.disc-fano",
                "2*2*2+(-1)",
                "Fano plane",
            ),
            (
                "std.capability.discrete.disc-part-p4",
                "3+2",
                "p(4)",
            ),
            (
                "std.capability.discrete.disc-p-five-three",
                "5*4*3",
                "P(5,3)",
            ),
            (
                "std.capability.discrete.disc-words-len",
                "2*2*2",
                "word count",
            ),
            (
                "std.capability.discrete.disc-nim-zero",
                "3+(-3)",
                "nim zero",
            ),
            (
                "std.capability.discrete.disc-collatz-odd",
                "3*5+1",
                "Collatz odd step",
            ),
            (
                "std.capability.discrete.disc-incl-excl",
                "5+5+5+(-2)+(-2)+(-2)+1",
                "inclusion-exclusion",
            ),
            (
                "std.capability.discrete.disc-central-six",
                "6*5*4/(3*2*1)",
                "central C(6,3)",
            ),
            (
                "std.capability.discrete.disc-design-bk",
                "4*3+(-6)*2",
                "design bk=vr",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    for catalog in [
        "discrete-mathematics-and-combinatorics-elementary-discrete-mathematics",
        "discrete-mathematics-and-combinatorics-enumerative-combinatorics",
        "discrete-mathematics-and-combinatorics-algebraic-combinatorics",
        "discrete-mathematics-and-combinatorics-extremal-combinatorics",
        "discrete-mathematics-and-combinatorics-probabilistic-combinatorics",
        "discrete-mathematics-and-combinatorics-additive-combinatorics",
        "discrete-mathematics-and-combinatorics-analytic-combinatorics",
        "discrete-mathematics-and-combinatorics-bijective-combinatorics",
        "discrete-mathematics-and-combinatorics-design-theory",
        "discrete-mathematics-and-combinatorics-matroid-theory",
        "discrete-mathematics-and-combinatorics-graph-theory",
        "discrete-mathematics-and-combinatorics-hypergraph-theory",
        "discrete-mathematics-and-combinatorics-spectral-graph-theory",
        "discrete-mathematics-and-combinatorics-topological-graph-theory",
        "discrete-mathematics-and-combinatorics-geometric-graph-theory",
        "discrete-mathematics-and-combinatorics-random-graph-theory",
        "discrete-mathematics-and-combinatorics-network-science",
        "discrete-mathematics-and-combinatorics-ramsey-theory",
        "discrete-mathematics-and-combinatorics-order-theory",
        "discrete-mathematics-and-combinatorics-lattice-theory",
        "discrete-mathematics-and-combinatorics-discrete-geometry",
        "discrete-mathematics-and-combinatorics-finite-geometry",
        "discrete-mathematics-and-combinatorics-combinatorial-geometry",
        "discrete-mathematics-and-combinatorics-tiling-theory",
        "discrete-mathematics-and-combinatorics-packing-and-covering",
        "discrete-mathematics-and-combinatorics-polyominoes-and-polyforms",
        "discrete-mathematics-and-combinatorics-combinatorial-games",
        "discrete-mathematics-and-combinatorics-recreational-mathematics",
        "discrete-mathematics-and-combinatorics-combinatorial-optimization",
        "discrete-mathematics-and-combinatorics-integer-partitions",
        "discrete-mathematics-and-combinatorics-permutation-patterns",
        "discrete-mathematics-and-combinatorics-word-combinatorics",
    ] {
        algebra_more_capability(&mut probe, &capability, catalog);
    }
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/discrete-ops.emath",
        &[
            "5 * 4 / 2",
            "6 * 5 * 4 / (4 * 3 * 2)",
            "7 + (-1)",
            "2 * 2 * 2 + (-1)",
            "5 * 4 * 3",
            "5 + 5 + 5 + (-2) + (-2) + (-2) + 1",
            "3 * 5 + 1",
        ],
    );
    probe.finish();
}

#[test]
fn probability_fields_depth() {
    let mut probe = Probe::new(
        "probability and statistics MATH: additivity, martingales, OLS, MC compute",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.probability.prb-add",
                "1+3",
                "additivity",
            ),
            (
                "std.capability.probability.prb-complement",
                "4+(-1)",
                "complement mass",
            ),
            (
                "std.capability.probability.prb-martingale",
                "5+(-5)",
                "martingale increment",
            ),
            (
                "std.capability.probability.prb-levy-scale",
                "(2*2)/2",
                "Levy scale",
            ),
            (
                "std.capability.probability.prb-perc",
                "2*5",
                "percolation bonds",
            ),
            (
                "std.capability.probability.prb-rand-graph",
                "5*4/2",
                "ER expected edges",
            ),
            (
                "std.capability.probability.prb-goe",
                "2*(2+1)/2",
                "GOE entries",
            ),
            (
                "std.capability.probability.prb-mmone",
                "3/(4-3)",
                "M/M/1 mean queue",
            ),
            (
                "std.capability.probability.prb-little",
                "2*5",
                "Little's law",
            ),
            (
                "std.capability.probability.prb-slope",
                "(6-2)/(4-2)",
                "OLS slope",
            ),
            (
                "std.capability.probability.prb-f1",
                "2*2*2/(2+2)",
                "F1 harmonic mean",
            ),
            (
                "std.capability.probability.prb-freq-mean",
                "(1+2+3+4+5)/5",
                "sample mean",
            ),
            (
                "std.capability.probability.prb-wmean",
                "(2*3+4*6)/(2+4)",
                "weighted mean",
            ),
            (
                "std.capability.probability.prb-chi2",
                "3*3+4*4",
                "chi-square two cells",
            ),
            (
                "std.capability.probability.prb-iqr",
                "7+(-3)",
                "IQR",
            ),
            (
                "std.capability.probability.prb-ate",
                "3+(-1)",
                "ATE",
            ),
            (
                "std.capability.probability.prb-pn",
                "6/3",
                "p/n ratio",
            ),
            (
                "std.capability.probability.prb-mc",
                "4/4",
                "MC hit rate",
            ),
            (
                "std.capability.probability.prb-binom-mean",
                "4*1/2",
                "binomial mean np",
            ),
            (
                "std.capability.probability.prb-die-sum",
                "1+2+3+4+5+6",
                "die face sum",
            ),
            (
                "std.capability.probability.prb-uq",
                "4+(-4)",
                "UQ residual",
            ),
            (
                "std.capability.probability.prb-cells",
                "2*3",
                "factorial cells",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    for catalog in [
        "probability-and-statistics-probability-theory",
        "probability-and-statistics-measure-theoretic-probability",
        "probability-and-statistics-stochastic-processes",
        "probability-and-statistics-markov-chains",
        "probability-and-statistics-markov-processes",
        "probability-and-statistics-martingale-theory",
        "probability-and-statistics-brownian-motion",
        "probability-and-statistics-l-vy-processes",
        "probability-and-statistics-random-fields",
        "probability-and-statistics-point-processes",
        "probability-and-statistics-stochastic-geometry",
        "probability-and-statistics-percolation-theory",
        "probability-and-statistics-random-graphs",
        "probability-and-statistics-random-matrix-theory",
        "probability-and-statistics-interacting-particle-systems",
        "probability-and-statistics-queueing-theory",
        "probability-and-statistics-reliability-theory",
        "probability-and-statistics-extreme-value-theory",
        "probability-and-statistics-bayesian-statistics",
        "probability-and-statistics-frequentist-statistics",
        "probability-and-statistics-likelihood-theory",
        "probability-and-statistics-statistical-decision-theory",
        "probability-and-statistics-causal-inference",
        "probability-and-statistics-experimental-design",
        "probability-and-statistics-regression-analysis",
        "probability-and-statistics-classification",
        "probability-and-statistics-time-series-analysis",
        "probability-and-statistics-spatial-statistics",
        "probability-and-statistics-multivariate-statistics",
        "probability-and-statistics-nonparametric-statistics",
        "probability-and-statistics-semiparametric-statistics",
        "probability-and-statistics-robust-statistics",
        "probability-and-statistics-high-dimensional-statistics",
        "probability-and-statistics-survival-analysis",
        "probability-and-statistics-statistical-learning-theory",
        "probability-and-statistics-uncertainty-quantification",
        "probability-and-statistics-monte-carlo-methods",
        "probability-and-statistics-sequential-analysis",
        "probability-and-statistics-imprecise-probability",
    ] {
        algebra_more_capability(&mut probe, &capability, catalog);
    }
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/probability-ops.emath",
        &[
            "1 + 3",
            "4 + (-1)",
            "1 + 2 + 3 + 4 + 5 + 6",
            "4 * 1 / 2",
            "5 + (-5)",
            "(2 * 2) / 2",
            "5 * 4 / 2",
            "2 * (2 + 1) / 2",
            "3 / (4 - 3)",
            "(6 - 2) / (4 - 2)",
            "2 * 2 * 2 / (2 + 2)",
            "(2 * 3 + 4 * 6) / (2 + 4)",
            "3 * 3 + 4 * 4",
            "4 + (-4)",
        ],
    );
    probe.finish();
}

#[test]
fn geometry_fields_depth() {
    let mut probe = Probe::new(
        "geometry and topology MATH: Pythagoras, Euler, Bezout, Minkowski compute",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.geometry.geo-pythag",
                "3*3+4*4",
                "Pythagoras 3-4-5",
            ),
            (
                "std.capability.geometry.geo-centroid",
                "(1+3+5)/3",
                "affine centroid",
            ),
            (
                "std.capability.geometry.geo-cross",
                "(2*6)/(3*4)",
                "projective cross-ratio",
            ),
            (
                "std.capability.geometry.geo-excess",
                "180+(-90)+(-60)+(-30)",
                "spherical excess",
            ),
            (
                "std.capability.geometry.geo-hyp",
                "3*3+(-2)*4",
                "hyperboloid slice",
            ),
            (
                "std.capability.geometry.geo-euler-c",
                "8+(-12)+6",
                "cube Euler",
            ),
            (
                "std.capability.geometry.geo-tetra",
                "4+(-6)+4",
                "tetrahedron Euler",
            ),
            (
                "std.capability.geometry.geo-bezout",
                "2*3",
                "Bezout intersections",
            ),
            (
                "std.capability.geometry.geo-mink",
                "1+1+1+(-1)",
                "Minkowski signature",
            ),
            (
                "std.capability.geometry.geo-contact",
                "2*2+1",
                "contact dim 2n+1",
            ),
            (
                "std.capability.geometry.geo-triang",
                "2*3+(-4)",
                "polygon triangulations",
            ),
            (
                "std.capability.geometry.geo-perim",
                "2+2+2+2",
                "square perimeter",
            ),
            (
                "std.capability.geometry.geo-chi-t2",
                "1+(-1)",
                "chi torus",
            ),
            (
                "std.capability.geometry.geo-trefoil",
                "3+2",
                "trefoil crossings",
            ),
            (
                "std.capability.geometry.geo-orb",
                "(3+2+1)/6",
                "orbifold Euler",
            ),
            (
                "std.capability.geometry.geo-ncg",
                "1+(-1)",
                "commutator slice",
            ),
            (
                "std.capability.geometry.geo-ddg",
                "1+(-1)+0",
                "discrete Laplacian",
            ),
            (
                "std.capability.geometry.geo-mid",
                "(1+5)/2",
                "convex midpoint",
            ),
            (
                "std.capability.geometry.geo-chi-s1",
                "1+(-1)+1",
                "S1 cell count",
            ),
            (
                "std.capability.geometry.geo-fisher",
                "2*2",
                "Fisher information",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    for catalog in [
        "geometry-and-topology-euclidean-geometry",
        "geometry-and-topology-affine-geometry",
        "geometry-and-topology-projective-geometry",
        "geometry-and-topology-spherical-geometry",
        "geometry-and-topology-hyperbolic-geometry",
        "geometry-and-topology-metric-geometry",
        "geometry-and-topology-convex-geometry",
        "geometry-and-topology-polyhedral-geometry",
        "geometry-and-topology-algebraic-geometry",
        "geometry-and-topology-complex-geometry",
        "geometry-and-topology-differential-geometry",
        "geometry-and-topology-riemannian-geometry",
        "geometry-and-topology-pseudo-riemannian-geometry",
        "geometry-and-topology-symplectic-geometry",
        "geometry-and-topology-contact-geometry",
        "geometry-and-topology-finsler-geometry",
        "geometry-and-topology-information-geometry",
        "geometry-and-topology-computational-geometry",
        "geometry-and-topology-fractal-geometry",
        "geometry-and-topology-geometric-measure-theory",
        "geometry-and-topology-integral-geometry",
        "geometry-and-topology-topology",
        "geometry-and-topology-general-topology",
        "geometry-and-topology-algebraic-topology",
        "geometry-and-topology-differential-topology",
        "geometry-and-topology-geometric-topology",
        "geometry-and-topology-low-dimensional-topology",
        "geometry-and-topology-knot-theory",
        "geometry-and-topology-homotopy-theory",
        "geometry-and-topology-homology-theory",
        "geometry-and-topology-cohomology-theory",
        "geometry-and-topology-manifold-theory",
        "geometry-and-topology-fiber-bundles",
        "geometry-and-topology-characteristic-classes",
        "geometry-and-topology-orbifolds",
        "geometry-and-topology-stacks",
        "geometry-and-topology-sheaf-theory",
        "geometry-and-topology-topos-theory",
        "geometry-and-topology-noncommutative-geometry",
        "geometry-and-topology-tropical-geometry",
        "geometry-and-topology-discrete-differential-geometry",
    ] {
        algebra_more_capability(&mut probe, &capability, catalog);
    }
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/geometry-ops.emath",
        &[
            "3 * 3 + 4 * 4",
            "(1 + 3 + 5) / 3",
            "(2 * 6) / (3 * 4)",
            "180 + (-90) + (-60) + (-30)",
            "3 * 3 + (-2) * 4",
            "8 + (-12) + 6",
            "1 + 1 + 1 + (-1)",
            "2 * 2 + 1",
            "2 * 3 + (-4)",
            "(3 + 2 + 1) / 6",
            "1 + (-1) + 0",
        ],
    );
    probe.finish();
}

#[test]
fn numerics_fields_depth() {
    let mut probe = Probe::new(
        "numerics optimization control MATH: det, trapezoid, stencil, Kalman compute",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            (
                "std.capability.numerics.num-det2",
                "2*5+(-3)*1",
                "2x2 determinant",
            ),
            (
                "std.capability.numerics.num-trap",
                "(1+3)*2/2",
                "trapezoid rule",
            ),
            (
                "std.capability.numerics.num-stencil",
                "1+(-2)+1",
                "second-difference stencil",
            ),
            (
                "std.capability.numerics.num-newton",
                "4/2",
                "Newton step",
            ),
            (
                "std.capability.numerics.num-jensen",
                "(2+8)/2",
                "convex Jensen",
            ),
            (
                "std.capability.numerics.num-energy",
                "3*3+4*4",
                "signal energy",
            ),
            (
                "std.capability.numerics.num-kalman",
                "6/3",
                "Kalman gain",
            ),
            (
                "std.capability.numerics.num-tsp",
                "3+4+5",
                "TSP tour",
            ),
            (
                "std.capability.numerics.num-gemv",
                "2*3+4",
                "gemv",
            ),
            (
                "std.capability.numerics.num-flux",
                "3+(-3)",
                "FVM flux cancel",
            ),
            (
                "std.capability.numerics.num-lp-slack",
                "4+(-4)",
                "LP slack",
            ),
            (
                "std.capability.numerics.num-makespan",
                "5+7",
                "schedule makespan",
            ),
            (
                "std.capability.numerics.num-nyquist",
                "8/2",
                "spectral Nyquist",
            ),
            (
                "std.capability.numerics.num-residual",
                "5+(-5)",
                "analysis residual",
            ),
            (
                "std.capability.numerics.num-width",
                "7+(-3)",
                "interval width",
            ),
            (
                "std.capability.numerics.num-flop",
                "2*3*4",
                "flop count",
            ),
            (
                "std.capability.numerics.num-horizon",
                "2*4",
                "MPC horizon",
            ),
            (
                "std.capability.numerics.num-eoq",
                "2*6",
                "EOQ",
            ),
            (
                "std.capability.numerics.num-pareto",
                "2*3+4*1",
                "weighted scalarization",
            ),
            (
                "std.capability.numerics.num-sim-mean",
                "(1+3+5)/3",
                "sim-opt mean",
            ),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    for catalog in [
        "numerics-optimization-operations-and-control-numerical-analysis",
        "numerics-optimization-operations-and-control-floating-point-analysis",
        "numerics-optimization-operations-and-control-interval-analysis",
        "numerics-optimization-operations-and-control-arbitrary-precision-arithmetic",
        "numerics-optimization-operations-and-control-numerical-linear-algebra",
        "numerics-optimization-operations-and-control-root-finding",
        "numerics-optimization-operations-and-control-interpolation",
        "numerics-optimization-operations-and-control-numerical-quadrature",
        "numerics-optimization-operations-and-control-numerical-differentiation",
        "numerics-optimization-operations-and-control-automatic-differentiation",
        "numerics-optimization-operations-and-control-finite-difference-methods",
        "numerics-optimization-operations-and-control-finite-element-methods",
        "numerics-optimization-operations-and-control-finite-volume-methods",
        "numerics-optimization-operations-and-control-boundary-element-methods",
        "numerics-optimization-operations-and-control-spectral-methods",
        "numerics-optimization-operations-and-control-meshfree-methods",
        "numerics-optimization-operations-and-control-multigrid-methods",
        "numerics-optimization-operations-and-control-sparse-numerical-methods",
        "numerics-optimization-operations-and-control-scientific-computing",
        "numerics-optimization-operations-and-control-high-performance-computing",
        "numerics-optimization-operations-and-control-optimization",
        "numerics-optimization-operations-and-control-convex-optimization",
        "numerics-optimization-operations-and-control-nonconvex-optimization",
        "numerics-optimization-operations-and-control-discrete-optimization",
        "numerics-optimization-operations-and-control-integer-programming",
        "numerics-optimization-operations-and-control-linear-programming",
        "numerics-optimization-operations-and-control-nonlinear-programming",
        "numerics-optimization-operations-and-control-stochastic-optimization",
        "numerics-optimization-operations-and-control-robust-optimization",
        "numerics-optimization-operations-and-control-multiobjective-optimization",
        "numerics-optimization-operations-and-control-global-optimization",
        "numerics-optimization-operations-and-control-derivative-free-optimization",
        "numerics-optimization-operations-and-control-optimal-control",
        "numerics-optimization-operations-and-control-model-predictive-control",
        "numerics-optimization-operations-and-control-variational-inequalities",
        "numerics-optimization-operations-and-control-operations-research",
        "numerics-optimization-operations-and-control-scheduling",
        "numerics-optimization-operations-and-control-routing",
        "numerics-optimization-operations-and-control-inventory-theory",
        "numerics-optimization-operations-and-control-network-optimization",
        "numerics-optimization-operations-and-control-simulation-optimization",
        "numerics-optimization-operations-and-control-decision-analysis",
        "numerics-optimization-operations-and-control-systems-theory",
        "numerics-optimization-operations-and-control-control-theory",
        "numerics-optimization-operations-and-control-estimation-and-filtering",
        "numerics-optimization-operations-and-control-signal-processing",
    ] {
        algebra_more_capability(&mut probe, &capability, catalog);
    }
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/numerics-ops.emath",
        &[
            "2 * 5 + (-3) * 1",
            "(1 + 3) * 2 / 2",
            "1 + (-2) + 1",
            "4 / 2",
            "(2 + 8) / 2",
            "3 * 3 + 4 * 4",
            "6 / 3",
            "3 + 4 + 5",
            "3 + (-3)",
            "4 + (-4)",
            "2 * 3 * 4",
            "(1 + 3 + 5) / 3",
        ],
    );
    probe.finish();
}

#[test]
fn physics_fields_depth() {
    let mut probe = Probe::new(
        "physics and engineering MATH: F=ma, Kepler, Ohm, E=mc^2 compute",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            ("std.capability.physics.phy-force", "2*3", "F=ma"),
            ("std.capability.physics.phy-ke", "2*2*2/2", "kinetic energy"),
            ("std.capability.physics.phy-lag", "5+(-2)", "Lagrangian T-V"),
            ("std.capability.physics.phy-kepler", "8/8", "Kepler slice"),
            ("std.capability.physics.phy-hooke", "3*5", "Hooke"),
            ("std.capability.physics.phy-cfl", "2*1/2", "CFL"),
            ("std.capability.physics.phy-re", "8*2/4", "Reynolds"),
            ("std.capability.physics.phy-ohm", "12/3", "Ohm"),
            ("std.capability.physics.phy-kirch", "2+3+(-5)", "KCL"),
            ("std.capability.physics.phy-emc", "2*3*3", "E=mc^2"),
            ("std.capability.physics.phy-an", "2+3", "A=Z+N"),
            ("std.capability.physics.phy-balance", "4+(-4)", "climate balance"),
            ("std.capability.physics.phy-ekin", "4*4/(2*2)", "p^2/2m"),
            ("std.capability.physics.phy-partz", "1+2+4", "partition function"),
            ("std.capability.physics.phy-torque", "2*4", "torque"),
            ("std.capability.physics.phy-da", "4/2", "Damkohler"),
            ("std.capability.physics.phy-yield", "6+(-6)", "yield residual"),
            ("std.capability.physics.phy-wave", "6/2", "wavelength"),
            ("std.capability.physics.phy-lattice", "2*2*2", "lattice"),
            ("std.capability.physics.phy-moment", "2*6", "bending moment"),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    for catalog in [
        "physics-and-engineering-mathematics-classical-mechanics",
        "physics-and-engineering-mathematics-analytical-mechanics",
        "physics-and-engineering-mathematics-celestial-mechanics",
        "physics-and-engineering-mathematics-continuum-mechanics",
        "physics-and-engineering-mathematics-solid-mechanics",
        "physics-and-engineering-mathematics-elasticity",
        "physics-and-engineering-mathematics-plasticity",
        "physics-and-engineering-mathematics-fracture-mechanics",
        "physics-and-engineering-mathematics-fluid-mechanics",
        "physics-and-engineering-mathematics-computational-fluid-dynamics",
        "physics-and-engineering-mathematics-turbulence",
        "physics-and-engineering-mathematics-aerodynamics",
        "physics-and-engineering-mathematics-hydrodynamics",
        "physics-and-engineering-mathematics-acoustics",
        "physics-and-engineering-mathematics-optics",
        "physics-and-engineering-mathematics-electromagnetism",
        "physics-and-engineering-mathematics-electromagnetic-theory",
        "physics-and-engineering-mathematics-plasma-physics",
        "physics-and-engineering-mathematics-thermodynamics",
        "physics-and-engineering-mathematics-heat-transfer",
        "physics-and-engineering-mathematics-statistical-mechanics",
        "physics-and-engineering-mathematics-condensed-matter-physics",
        "physics-and-engineering-mathematics-quantum-mechanics",
        "physics-and-engineering-mathematics-quantum-field-theory",
        "physics-and-engineering-mathematics-quantum-information",
        "physics-and-engineering-mathematics-relativity",
        "physics-and-engineering-mathematics-general-relativity",
        "physics-and-engineering-mathematics-gravitational-waves",
        "physics-and-engineering-mathematics-particle-physics",
        "physics-and-engineering-mathematics-nuclear-physics",
        "physics-and-engineering-mathematics-astronomy",
        "physics-and-engineering-mathematics-astrophysics",
        "physics-and-engineering-mathematics-cosmology",
        "physics-and-engineering-mathematics-geophysics",
        "physics-and-engineering-mathematics-seismology",
        "physics-and-engineering-mathematics-geodesy",
        "physics-and-engineering-mathematics-atmospheric-science",
        "physics-and-engineering-mathematics-oceanography",
        "physics-and-engineering-mathematics-climate-science",
        "physics-and-engineering-mathematics-materials-science",
        "physics-and-engineering-mathematics-structural-engineering",
        "physics-and-engineering-mathematics-electrical-engineering-mathematics",
        "physics-and-engineering-mathematics-mechanical-engineering-mathematics",
        "physics-and-engineering-mathematics-chemical-engineering-mathematics",
    ] {
        algebra_more_capability(&mut probe, &capability, catalog);
    }
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/physics-ops.emath",
        &[
            "2 * 3",
            "2 * 2 * 2 / 2",
            "5 + (-2)",
            "8 / 8",
            "3 * 5",
            "2 * 1 / 2",
            "8 * 2 / 4",
            "12 / 3",
            "2 + 3 + (-5)",
            "2 * 3 * 3",
            "4 * 4 / (2 * 2)",
            "6 + (-6)",
        ],
    );
    probe.finish();
}

#[test]
fn rest_of_bucket_b_fields_depth() {
    let mut probe = Probe::new(
        "rest of MATH bucket B: CS, foundations, bio, econ, cryptology, foundation-models",
    );
    let blocks = spec_feature_blocks();
    require_ids(
        &mut probe,
        &blocks,
        &[
            ("std.capability.cs.cs-binsearch", "(1+5)/2", "binary search"),
            ("std.capability.cs.cs-gcd", "15/3", "Euclid"),
            ("std.capability.cs.cs-ham", "2*3+1", "Hamming"),
            ("std.capability.cs.cs-loss", "4+(-4)", "ML loss"),
            ("std.capability.foundations.fnd-and", "1*1", "classical AND"),
            ("std.capability.foundations.fnd-tertium", "1+1+1", "three-valued"),
            ("std.capability.foundations.fnd-card", "2*2", "cardinal"),
            ("std.capability.foundations.fnd-cut", "3+(-1)", "cut-elim"),
            ("std.capability.bio.bio-lotka", "2*3+(-1)*4", "Lotka-Volterra"),
            ("std.capability.bio.bio-voxel", "2*2*2", "imaging voxels"),
            ("std.capability.bio.bio-hr", "4*15", "physiology"),
            ("std.capability.econ.ecn-nash", "2*2", "game matrix"),
            ("std.capability.econ.ecn-shapley", "(1+2)/3", "Shapley"),
            ("std.capability.econ.ecn-ols", "(6-2)/(4-2)", "econometric slope"),
            ("std.capability.econ.ecn-fair", "6/3", "fair split"),
            ("std.capability.cryptology.cry-affine", "5*3+7", "affine cipher"),
            ("std.capability.cryptology.cry-grid", "9*9", "sudoku"),
            ("std.capability.cryptology.cry-share", "2+1", "secret sharing"),
            (
                "std.capability.foundation.models.fma-attn",
                "4*4",
                "attention",
            ),
            ("std.capability.foundation.models.fma-gqa", "8/2", "GQA"),
            ("std.capability.foundation.models.fma-nll", "4+(-4)", "NLL"),
            ("std.capability.foundation.models.fma-lora", "2*8", "LoRA"),
        ],
        true,
    );
    let capability = workspace_file("language/CAPABILITY.md");
    for catalog in [
        "computer-science-information-and-ai-algorithms",
        "computer-science-information-and-ai-data-structures",
        "computer-science-information-and-ai-computational-complexity",
        "computer-science-information-and-ai-automata-theory",
        "computer-science-information-and-ai-formal-languages",
        "computer-science-information-and-ai-programming-languages",
        "computer-science-information-and-ai-compiler-theory",
        "computer-science-information-and-ai-denotational-semantics",
        "computer-science-information-and-ai-operational-semantics",
        "computer-science-information-and-ai-axiomatic-semantics",
        "computer-science-information-and-ai-type-systems",
        "computer-science-information-and-ai-term-rewriting",
        "computer-science-information-and-ai-symbolic-computation",
        "computer-science-information-and-ai-computer-algebra",
        "computer-science-information-and-ai-formal-verification",
        "computer-science-information-and-ai-model-checking",
        "computer-science-information-and-ai-sat-solving",
        "computer-science-information-and-ai-smt-solving",
        "computer-science-information-and-ai-constraint-programming",
        "computer-science-information-and-ai-abstract-interpretation",
        "computer-science-information-and-ai-symbolic-execution",
        "computer-science-information-and-ai-databases",
        "computer-science-information-and-ai-distributed-systems",
        "computer-science-information-and-ai-concurrent-systems",
        "computer-science-information-and-ai-real-time-systems",
        "computer-science-information-and-ai-computer-networks",
        "computer-science-information-and-ai-information-theory",
        "computer-science-information-and-ai-coding-theory",
        "computer-science-information-and-ai-data-compression",
        "computer-science-information-and-ai-communication-theory",
        "computer-science-information-and-ai-circuit-theory",
        "computer-science-information-and-ai-cryptography",
        "computer-science-information-and-ai-quantum-computing",
        "computer-science-information-and-ai-computer-graphics",
        "computer-science-information-and-ai-scientific-visualization",
        "computer-science-information-and-ai-artificial-intelligence",
        "computer-science-information-and-ai-machine-learning",
        "computer-science-information-and-ai-deep-learning",
        "computer-science-information-and-ai-reinforcement-learning",
        "computer-science-information-and-ai-causal-machine-learning",
        "computer-science-information-and-ai-probabilistic-programming",
        "computer-science-information-and-ai-differentiable-programming",
        "computer-science-information-and-ai-neuro-symbolic-systems",
        "computer-science-information-and-ai-agent-systems",
        "foundations-logic-and-mathematical-knowledge-mathematical-logic",
        "foundations-logic-and-mathematical-knowledge-classical-logic",
        "foundations-logic-and-mathematical-knowledge-intuitionistic-logic",
        "foundations-logic-and-mathematical-knowledge-modal-logic",
        "foundations-logic-and-mathematical-knowledge-temporal-logic",
        "foundations-logic-and-mathematical-knowledge-epistemic-logic",
        "foundations-logic-and-mathematical-knowledge-linear-logic",
        "foundations-logic-and-mathematical-knowledge-relevance-logic",
        "foundations-logic-and-mathematical-knowledge-paraconsistent-logic",
        "foundations-logic-and-mathematical-knowledge-many-valued-logic",
        "foundations-logic-and-mathematical-knowledge-fuzzy-logic",
        "foundations-logic-and-mathematical-knowledge-probabilistic-logic",
        "foundations-logic-and-mathematical-knowledge-algebraic-logic",
        "foundations-logic-and-mathematical-knowledge-categorical-logic",
        "foundations-logic-and-mathematical-knowledge-set-theory",
        "foundations-logic-and-mathematical-knowledge-descriptive-set-theory",
        "foundations-logic-and-mathematical-knowledge-large-cardinal-theory",
        "foundations-logic-and-mathematical-knowledge-forcing-and-independence",
        "foundations-logic-and-mathematical-knowledge-type-theory",
        "foundations-logic-and-mathematical-knowledge-dependent-type-theory",
        "foundations-logic-and-mathematical-knowledge-homotopy-type-theory",
        "foundations-logic-and-mathematical-knowledge-proof-theory",
        "foundations-logic-and-mathematical-knowledge-model-theory",
        "foundations-logic-and-mathematical-knowledge-finite-model-theory",
        "foundations-logic-and-mathematical-knowledge-computability-theory",
        "foundations-logic-and-mathematical-knowledge-recursion-theory",
        "foundations-logic-and-mathematical-knowledge-constructive-mathematics",
        "foundations-logic-and-mathematical-knowledge-reverse-mathematics",
        "foundations-logic-and-mathematical-knowledge-nonstandard-analysis",
        "foundations-logic-and-mathematical-knowledge-formal-theorem-proving",
        "foundations-logic-and-mathematical-knowledge-mathematical-knowledge-management",
        "foundations-logic-and-mathematical-knowledge-mathematical-modeling",
        "foundations-logic-and-mathematical-knowledge-simulation-theory",
        "biology-chemistry-medicine-and-earth-systems-mathematical-biology",
        "biology-chemistry-medicine-and-earth-systems-population-dynamics",
        "biology-chemistry-medicine-and-earth-systems-ecology",
        "biology-chemistry-medicine-and-earth-systems-evolutionary-dynamics",
        "biology-chemistry-medicine-and-earth-systems-population-genetics",
        "biology-chemistry-medicine-and-earth-systems-quantitative-genetics",
        "biology-chemistry-medicine-and-earth-systems-genomics",
        "biology-chemistry-medicine-and-earth-systems-bioinformatics",
        "biology-chemistry-medicine-and-earth-systems-systems-biology",
        "biology-chemistry-medicine-and-earth-systems-biochemical-reaction-networks",
        "biology-chemistry-medicine-and-earth-systems-mathematical-neuroscience",
        "biology-chemistry-medicine-and-earth-systems-computational-neuroscience",
        "biology-chemistry-medicine-and-earth-systems-epidemiology",
        "biology-chemistry-medicine-and-earth-systems-immunology-modeling",
        "biology-chemistry-medicine-and-earth-systems-cancer-modeling",
        "biology-chemistry-medicine-and-earth-systems-physiology",
        "biology-chemistry-medicine-and-earth-systems-biomechanics",
        "biology-chemistry-medicine-and-earth-systems-morphogenesis",
        "biology-chemistry-medicine-and-earth-systems-pharmacokinetics",
        "biology-chemistry-medicine-and-earth-systems-pharmacodynamics",
        "biology-chemistry-medicine-and-earth-systems-medical-imaging-mathematics",
        "biology-chemistry-medicine-and-earth-systems-public-health-modeling",
        "biology-chemistry-medicine-and-earth-systems-mathematical-chemistry",
        "biology-chemistry-medicine-and-earth-systems-chemical-kinetics",
        "biology-chemistry-medicine-and-earth-systems-reaction-network-theory",
        "biology-chemistry-medicine-and-earth-systems-molecular-modeling",
        "biology-chemistry-medicine-and-earth-systems-quantum-chemistry",
        "biology-chemistry-medicine-and-earth-systems-computational-chemistry",
        "biology-chemistry-medicine-and-earth-systems-earth-system-science",
        "biology-chemistry-medicine-and-earth-systems-environmental-modeling",
        "biology-chemistry-medicine-and-earth-systems-hydrology",
        "biology-chemistry-medicine-and-earth-systems-cryosphere-modeling",
        "biology-chemistry-medicine-and-earth-systems-soil-and-porous-media-modeling",
        "biology-chemistry-medicine-and-earth-systems-planetary-science",
        "economics-social-systems-games-and-decisions-game-theory",
        "economics-social-systems-games-and-decisions-cooperative-game-theory",
        "economics-social-systems-games-and-decisions-noncooperative-game-theory",
        "economics-social-systems-games-and-decisions-evolutionary-game-theory",
        "economics-social-systems-games-and-decisions-mechanism-design",
        "economics-social-systems-games-and-decisions-auction-theory",
        "economics-social-systems-games-and-decisions-social-choice-theory",
        "economics-social-systems-games-and-decisions-decision-theory",
        "economics-social-systems-games-and-decisions-behavioral-decision-theory",
        "economics-social-systems-games-and-decisions-economics",
        "economics-social-systems-games-and-decisions-microeconomics",
        "economics-social-systems-games-and-decisions-macroeconomics",
        "economics-social-systems-games-and-decisions-econometrics",
        "economics-social-systems-games-and-decisions-mathematical-finance",
        "economics-social-systems-games-and-decisions-actuarial-mathematics",
        "economics-social-systems-games-and-decisions-risk-theory",
        "economics-social-systems-games-and-decisions-market-microstructure",
        "economics-social-systems-games-and-decisions-operations-economics",
        "economics-social-systems-games-and-decisions-network-economics",
        "economics-social-systems-games-and-decisions-demography",
        "economics-social-systems-games-and-decisions-mathematical-sociology",
        "economics-social-systems-games-and-decisions-political-modeling",
        "economics-social-systems-games-and-decisions-agent-based-social-simulation",
        "economics-social-systems-games-and-decisions-complex-systems",
        "economics-social-systems-games-and-decisions-collective-intelligence",
        "economics-social-systems-games-and-decisions-fairness-and-allocation",
        "cryptology-coding-puzzles-and-hidden-state-systems-cryptology",
        "cryptology-coding-puzzles-and-hidden-state-systems-classical-ciphers",
        "cryptology-coding-puzzles-and-hidden-state-systems-symmetric-cryptography",
        "cryptology-coding-puzzles-and-hidden-state-systems-public-key-cryptography",
        "cryptology-coding-puzzles-and-hidden-state-systems-post-quantum-cryptography",
        "cryptology-coding-puzzles-and-hidden-state-systems-cryptanalysis",
        "cryptology-coding-puzzles-and-hidden-state-systems-protocol-security",
        "cryptology-coding-puzzles-and-hidden-state-systems-randomness-and-entropy",
        "cryptology-coding-puzzles-and-hidden-state-systems-secret-sharing",
        "cryptology-coding-puzzles-and-hidden-state-systems-commitment-schemes",
        "cryptology-coding-puzzles-and-hidden-state-systems-zero-knowledge-systems",
        "cryptology-coding-puzzles-and-hidden-state-systems-secure-multiparty-computation",
        "cryptology-coding-puzzles-and-hidden-state-systems-steganography",
        "cryptology-coding-puzzles-and-hidden-state-systems-constraint-satisfaction",
        "cryptology-coding-puzzles-and-hidden-state-systems-logic-puzzles",
        "cryptology-coding-puzzles-and-hidden-state-systems-grid-puzzles",
        "cryptology-coding-puzzles-and-hidden-state-systems-cryptograms",
        "cryptology-coding-puzzles-and-hidden-state-systems-alphametics",
        "cryptology-coding-puzzles-and-hidden-state-systems-word-puzzles",
        "cryptology-coding-puzzles-and-hidden-state-systems-number-puzzles",
        "cryptology-coding-puzzles-and-hidden-state-systems-graph-and-path-puzzles",
        "cryptology-coding-puzzles-and-hidden-state-systems-geometry-puzzles",
        "cryptology-coding-puzzles-and-hidden-state-systems-proof-puzzles",
        "cryptology-coding-puzzles-and-hidden-state-systems-puzzle-generation",
        "cryptology-coding-puzzles-and-hidden-state-systems-puzzle-difficulty-theory",
        "cryptology-coding-puzzles-and-hidden-state-systems-hidden-state-systems",
        "cryptology-coding-puzzles-and-hidden-state-systems-knowledge-geometry",
        "cryptology-coding-puzzles-and-hidden-state-systems-access-geometry",
        "cryptology-coding-puzzles-and-hidden-state-systems-security-games",
        "foundation-models-and-ai-systems-tokenization",
        "foundation-models-and-ai-systems-data-curation-for-models",
        "foundation-models-and-ai-systems-transformer-architecture",
        "foundation-models-and-ai-systems-attention-mechanisms",
        "foundation-models-and-ai-systems-grouped-query-attention",
        "foundation-models-and-ai-systems-multi-query-attention",
        "foundation-models-and-ai-systems-multi-head-latent-attention",
        "foundation-models-and-ai-systems-mixture-of-experts-models",
        "foundation-models-and-ai-systems-state-space-sequence-models",
        "foundation-models-and-ai-systems-hybrid-sequence-models",
        "foundation-models-and-ai-systems-language-model-objectives",
        "foundation-models-and-ai-systems-model-pretraining",
        "foundation-models-and-ai-systems-distributed-training",
        "foundation-models-and-ai-systems-model-checkpointing",
        "foundation-models-and-ai-systems-model-post-training",
        "foundation-models-and-ai-systems-model-distillation",
        "foundation-models-and-ai-systems-parameter-efficient-adaptation",
        "foundation-models-and-ai-systems-model-evaluation",
        "foundation-models-and-ai-systems-model-quantization",
        "foundation-models-and-ai-systems-model-serving",
        "foundation-models-and-ai-systems-retrieval-augmented-generation",
        "foundation-models-and-ai-systems-multimodal-foundation-models",
        "foundation-models-and-ai-systems-agentic-model-systems",
        "foundation-models-and-ai-systems-model-lifecycle-provenance",
    ] {
        algebra_more_capability(&mut probe, &capability, catalog);
    }
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/cs-ops.emath",
        &["(1 + 5) / 2", "15 / 3", "2 * 3 + 1", "4 + (-4)", "4 * 8"],
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/foundations-ops.emath",
        &["1 * 1", "1 + 1 + 1", "(1 + 3) / 2", "3 + (-1)"],
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/bio-ops.emath",
        &["2 * 3 + (-1) * 4", "2 * 2 * 2", "4 * 15", "3 + (-3)"],
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/econ-ops.emath",
        &["2 * 2", "(1 + 2) / 3", "(6 - 2) / (4 - 2)", "6 / 3"],
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/cryptology-ops.emath",
        &["5 * 3 + 7", "9 * 9", "2 + 1", "8 / 2"],
    );
    require_fixture(
        &mut probe,
        "tests/fixtures/v20/foundation-models-ops.emath",
        &["4 * 4", "8 / 2", "4 + (-4)", "2 * 8"],
    );
    probe.finish();
}

//! Teaching programs that exist must compute, not leftover `x=N`.
//!
//! `examples/` is a curated teaching set, not a per-capability inventory.
//! This probe does not require a teaching file for every family or heading.
//! It fails if an existing teaching program assigns a primary output a bare
//! literal, or if complement regresses to writing the answer.

use emath_test_harness::{Probe, workspace_file, workspace_path};

fn family_rows(capability: &str) -> Vec<(String, String)> {
    let pre = capability
        .split("## Catalog heading coverage")
        .next()
        .expect("family slices precede heading coverage");
    let mut rows = Vec::new();
    for line in pre.lines() {
        let Some(row) = parse_example_row(line) else {
            continue;
        };
        rows.push(row);
    }
    rows
}

fn parse_example_row(line: &str) -> Option<(String, String)> {
    if !line.starts_with("| ") || line.contains("Example") || line.contains("---|") {
        return None;
    }
    let cells: Vec<&str> = line.split('|').map(str::trim).collect();
    let name = cells.get(1)?.to_string();
    let example = cells.get(2)?;
    if !example.starts_with('`') || !example.ends_with(".emath`") {
        return None;
    }
    Some((name, example.trim_matches('`').to_string()))
}

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

#[test]
fn family_examples_compute_named_identities() {
    let mut probe = Probe::new(
        "existing teaching programs compute (not leftover x=N); examples/ is not a catalog",
    );
    let capability = workspace_file("language/CAPABILITY.md");
    let families = family_rows(&capability);

    for (_name, rel) in families.iter() {
        let example = workspace_path(&format!("language/examples/{rel}"));
        if !example.is_file() {
            continue;
        }
        let source = std::fs::read_to_string(&example).expect("read example");
        for (output, rhs) in leftover_output_assigns(&source) {
            probe.fail(
                format!("leftover-{rel}-{output}"),
                format!("{rel} assigns leftover {output} = {rhs} instead of computing"),
            );
        }
    }

    let complement = workspace_file("language/examples/probability/complement.emath");
    probe.demand(
        "complement-not-answer",
        !leftover_output_assigns(&complement)
            .iter()
            .any(|(name, rhs)| name == "q" && rhs == "rat(3, 4)"),
        "complement leftover x=N: q = rat(3, 4)",
    );
    probe.contains(
        "complement-computes-one-minus-p",
        &complement,
        "rat_add(rat(1, 1), rat(-1, 4))",
    );

    probe.finish();
}

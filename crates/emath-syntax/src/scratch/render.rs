//! Synthesized declaration rendering and name allocation.

use super::*;

pub(super) fn render_function(
    name: &str,
    header: Option<&str>,
    inputs: &[String],
    defs: &[(String, String)],
    examples: &[(String, String)],
    compile: Option<&(String, String)>,
    comments: &[String],
    goals: &[(String, String)],
) -> String {
    let header = header.map_or_else(|| format!("emath function {name}:"), ToString::to_string);
    render_from_header(&header, inputs, defs, examples, compile, comments, goals)
}

pub(super) fn render_from_header(
    header: &str,
    inputs: &[String],
    defs: &[(String, String)],
    examples: &[(String, String)],
    compile: Option<&(String, String)>,
    comments: &[String],
    goals: &[(String, String)],
) -> String {
    let mut out = String::new();
    for comment in comments {
        out.push_str(comment);
        out.push('\n');
    }
    out.push_str(header);
    if !header.ends_with(':') {
        out.push(':');
    }
    out.push('\n');
    if !inputs.is_empty() {
        out.push_str("    inputs:\n");
        for name in inputs {
            out.push_str("        ");
            out.push_str(name);
            out.push('\n');
        }
        out.push('\n');
    }
    // Every defined name that is not an input is an output of the
    // component: the L3 contract makes the produced surface explicit
    // instead of leaning on the evaluate-everything default. Outputs
    // must be typed (`name: Type`), so the untyped default mirrors the
    // inputs rule (Float64, N-TYPE-001) in the emitted text itself.
    // E-SEC-130 (R6): a contract with `outputs:` but no `inputs:` is
    // refused, so the synthesized `outputs:` section is emitted only
    // when the pane actually declared inputs. A bare pane computation
    // has no I/O surface to name; it stays a plain definitions block.
    let mut outputs: Vec<&str> = Vec::new();
    if !inputs.is_empty() {
        for (name, _) in defs {
            if inputs.iter().any(|input| input == name) {
                continue;
            }
            if !outputs.contains(&name.as_str()) {
                outputs.push(name.as_str());
            }
        }
    }
    if !outputs.is_empty() {
        out.push_str("    outputs:\n");
        for name in &outputs {
            out.push_str("        ");
            out.push_str(name);
            out.push_str(": Float64\n");
        }
        out.push('\n');
    }
    out.push_str("    definitions:\n");
    if defs.is_empty() {
        out.push_str("        result = 0\n");
    } else {
        for (name, rhs) in defs {
            out.push_str("        ");
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(rhs);
            out.push('\n');
        }
    }
    if let Some((lang, profile)) = compile {
        out.push('\n');
        out.push_str("    compile:\n");
        out.push_str("        target ");
        out.push_str(lang);
        out.push('\n');
        out.push_str("        profile ");
        out.push_str(profile);
        out.push('\n');
        out.push_str("        numeric strict-f64\n");
    }
    if !goals.is_empty() {
        out.push('\n');
        out.push_str("    goals:\n");
        for (kind, target) in goals {
            out.push_str("        ");
            out.push_str(kind);
            out.push_str(" <");
            out.push_str(target);
            out.push_str(">:\n");
            out.push_str("            produce rust.library\n");
        }
    }
    if !examples.is_empty() {
        out.push('\n');
        out.push_str("    tests:\n");
        for (name, value) in examples {
            out.push_str("        example <");
            out.push_str(name);
            out.push_str("_example>:\n");
            out.push_str("            given ");
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(value);
            out.push('\n');
        }
    }
    out
}

pub(super) fn free_names(
    defs: &[(String, String)],
    examples: &[(String, String)],
    assigned: &[&str],
) -> Vec<String> {
    let mut free = Vec::new();
    let mut scan = |text: &str| {
        let mut used = Vec::new();
        let mut bound = Vec::new();
        collect_names(text, &mut used, &mut bound);
        for ident in used {
            if !assigned.contains(&ident.as_str())
                && !bound.iter().any(|b| b == &ident)
                && !is_builtin(&ident)
                && !free.iter().any(|f| f == &ident)
            {
                free.push(ident);
            }
        }
    };
    for (_, rhs) in defs {
        scan(rhs);
    }
    for (_, value) in examples {
        scan(value);
    }
    free
}

pub(super) fn collect_names(text: &str, used: &mut Vec<String>, bound: &mut Vec<String>) {
    let idents = scan_idents(text);
    for name in binder_names(&idents) {
        if !bound.iter().any(|b| b == name) {
            bound.push(name.to_string());
        }
    }
    for ident in idents {
        if !used.iter().any(|u| u == ident) {
            used.push(ident.to_string());
        }
    }
}

pub(super) fn first_free_ident(text: &str) -> Option<&str> {
    scan_idents(text)
        .into_iter()
        .find(|ident| !is_builtin(ident))
}

pub(super) fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            let prev = index.checked_sub(1).and_then(|pos| bytes.get(pos)).copied();
            let next = bytes.get(index + 1).copied();
            if matches!(prev, Some(b'!' | b'<' | b'>' | b'=')) || next == Some(b'=') {
                index += 1;
                continue;
            }
            return Some((&line[..index], &line[index + 1..]));
        }
        index += 1;
    }
    None
}

pub(super) fn is_ident(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(super) fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

pub(super) fn is_binder_head(name: &str) -> bool {
    matches!(name, "sum" | "product" | "integral" | "forall" | "exists")
}

pub(super) fn binder_names<'a>(idents: &[&'a str]) -> Vec<&'a str> {
    let mut names = Vec::new();
    let mut index = 0;
    while index + 2 < idents.len() {
        if is_binder_head(idents[index]) && idents[index + 2] == "in" {
            names.push(idents[index + 1]);
            index += 3;
            continue;
        }
        index += 1;
    }
    names
}

pub(super) fn scan_idents(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut idents = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let b = bytes[index];
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = index;
            index = skip_word(bytes, index);
            idents.push(&text[start..index]);
            continue;
        }
        if b.is_ascii_digit() {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_digit()
                    || bytes[index] == b'.'
                    || bytes[index] == b'e'
                    || bytes[index] == b'E'
                    || bytes[index] == b'+'
                    || bytes[index] == b'-')
            {
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    idents
}

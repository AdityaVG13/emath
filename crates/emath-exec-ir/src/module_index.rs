//! Discovery index of ordinary modules: every exported name under the
//! `language/modules` and `language/domains` roots, with the import path
//! a user writes, a signature, a one-line summary, and its location.
//!
//! The index reads the same files and the same `exports:` surface as
//! import resolution. Objects are always listed (types are global);
//! functions and queries only when their file exports them. A file whose
//! path an earlier root already answered is skipped, because import
//! resolution would never reach it.
//!
//! Summary: the comment block directly under the declaration head; else
//! the file header comment sentences that mention the name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use emath_core::tree::{Declaration, Item, StmtKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleExport {
    /// Import path as written after `use` (`algebra.linalg`).
    pub module: String,
    /// `modules` (tools) or `domains` (objects of study).
    pub shelf: &'static str,
    pub name: String,
    /// `function`, `query`, or `object`.
    pub kind: String,
    pub signature: String,
    pub summary: String,
    pub file: PathBuf,
    /// 1-based line of the declaration head.
    pub line: usize,
}

impl ModuleExport {
    /// Case-insensitive match on name, import path, or summary.
    #[must_use]
    pub fn matches(&self, search: &str) -> bool {
        let needle = search.to_lowercase();
        [&self.name, &self.module, &self.summary]
            .iter()
            .any(|text| text.to_lowercase().contains(&needle))
    }
}

/// Exports of every module under `roots`, in root order then path order.
/// Files that do not parse are skipped: the index is discovery, and
/// `emath check` on that file reports the parse error.
#[must_use]
pub fn module_exports(roots: &[PathBuf]) -> Vec<ModuleExport> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for root in roots {
        let shelf = if root.file_name().is_some_and(|name| name == "domains") {
            "domains"
        } else {
            "modules"
        };
        let mut files = Vec::new();
        collect_emath(root, &mut files);
        files.sort();
        for file in files {
            let Ok(relative) = file.strip_prefix(root) else {
                continue;
            };
            let module = relative
                .with_extension("")
                .iter()
                .map(|part| part.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(".");
            if !seen.insert(module.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            index_file(&text, &module, shelf, &file, &mut out);
        }
    }
    out
}

fn collect_emath(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_emath(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "emath") {
            out.push(path);
        }
    }
}

fn index_file(text: &str, module: &str, shelf: &'static str, file: &Path, out: &mut Vec<ModuleExport>) {
    let (tree, diagnostics) = emath_syntax::parse_str(text);
    if diagnostics.has_errors() {
        return;
    }
    let exports: BTreeSet<&str> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Exports { names, .. } => Some(names.iter().map(String::as_str)),
            _ => None,
        })
        .flatten()
        .collect();
    let header = text
        .lines()
        .take_while(|line| line.trim_start().starts_with('#'))
        .map(comment_text)
        .collect::<Vec<_>>()
        .join(" ");
    // Header prose wraps mid-sentence, so the fallback picks sentences.
    let sentences: Vec<&str> = header
        .split_inclusive(". ")
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        let object = decl.as_kind == "object";
        if !object && !exports.contains(decl.name.as_str()) {
            continue;
        }
        let start = decl.head_source.start as usize;
        let mut summary = body_comment(text, start);
        if summary.is_empty() {
            summary = sentences
                .iter()
                .filter(|sentence| mentions(sentence, &decl.name))
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
        }
        out.push(ModuleExport {
            module: module.to_string(),
            shelf,
            name: decl.name.clone(),
            kind: decl.as_kind.clone(),
            signature: signature(text, decl),
            summary,
            file: file.to_path_buf(),
            line: text[..start.min(text.len())].matches('\n').count() + 1,
        });
    }
}

fn comment_text(line: &str) -> &str {
    line.trim_start().trim_start_matches('#').trim()
}

/// Contiguous comment lines right after the head line (the head
/// span covers the whole declaration, so only its start is used).
fn body_comment(text: &str, head_start: usize) -> String {
    let rest = text.get(head_start..).unwrap_or("");
    rest.lines()
        .skip(1)
        .take_while(|line| line.trim_start().starts_with('#'))
        .map(comment_text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn mentions(line: &str, name: &str) -> bool {
    line.match_indices(name).any(|(at, _)| {
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        let before = line[..at].chars().next_back().is_none_or(|c| !ident(c));
        let after = line[at + name.len()..].chars().next().is_none_or(|c| !ident(c));
        before && after
    })
}

fn signature(text: &str, decl: &Declaration) -> String {
    let fields = |section: &str| -> Vec<String> {
        decl.sections()
            .filter(|s| s.name == section)
            .flat_map(|s| s.suite.statements.iter())
            .filter(|stmt| matches!(stmt.kind, StmtKind::FieldDecl { .. }))
            .map(|stmt| slice(text, stmt.source.start, stmt.source.end).trim().to_string())
            .collect()
    };
    if decl.as_kind == "object" {
        let fields = fields("representation");
        return if fields.is_empty() {
            decl.name.clone()
        } else {
            format!("{} {{ {} }}", decl.name, fields.join(", "))
        };
    }
    let inputs = fields("inputs").join(", ");
    let outputs = fields("outputs");
    let name = if decl.generics.is_empty() {
        decl.name.clone()
    } else {
        let params: Vec<&str> = decl.generics.iter().map(|g| g.name.as_str()).collect();
        format!("{}<{}>", decl.name, params.join(", "))
    };
    match outputs.len() {
        0 => format!("{name}({inputs})"),
        1 => format!("{name}({inputs}) -> {}", outputs[0]),
        _ => format!("{name}({inputs}) -> ({})", outputs.join(", ")),
    }
}

fn slice(text: &str, start: u32, end: u32) -> &str {
    text.get(start as usize..end as usize).unwrap_or("")
}

/// Markdown catalog grouped by shelf and area (first import segment).
#[must_use]
pub fn render_catalog(exports: &[ModuleExport]) -> String {
    let mut out = String::from(
        "<!-- @generated by `cargo xtask generate-language` from language/modules and language/domains; DO NOT EDIT -->\n# Module catalog\n\nEvery exported name, by shelf and area. Search it live with `emath api --search <text>`.\n",
    );
    let mut shelf = "";
    let mut area = String::new();
    let mut module = "";
    for export in exports {
        if export.shelf != shelf {
            shelf = export.shelf;
            area.clear();
            out.push_str(&format!("\n## {shelf}/\n"));
        }
        let this_area = export.module.split('.').next().unwrap_or("").to_string();
        if this_area != area {
            area = this_area;
            out.push_str(&format!("\n### {area}\n"));
        }
        if export.module != module {
            module = &export.module;
            out.push_str(&format!("\n`use {module}`\n\n"));
        }
        let summary = if export.summary.is_empty() {
            String::new()
        } else {
            format!(": {}", export.summary.replace('|', "\\|"))
        };
        out.push_str(&format!("- `{}`{summary}\n", export.signature));
    }
    out
}

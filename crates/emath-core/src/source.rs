//! Source files, line maps and human-readable diagnostic rendering.
//!
//! Tier 0 foundation: moved from `emath-source` (which now re-exports these
//! types) because they depend only on core identity/diagnostic types
//! (`FileId`, `Diagnostic`) and are part of the compiler-session state every
//! front-end crate shares.

use crate::{Diagnostic, FileId};
use std::collections::HashMap;
use std::path::Path;

/// One loaded source file with its line-start table.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: FileId,
    /// Display name (path as given by the caller).
    pub name: String,
    pub text: String,
    /// Byte offset of the start of each line (line 0 starts at 0).
    line_starts: Vec<u32>,
}

impl SourceFile {
    fn build(text: String, id: FileId, name: String) -> Self {
        let mut line_starts = Vec::new();
        line_starts.push(0);
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(u32::try_from(index + 1).unwrap_or(u32::MAX));
            }
        }
        Self {
            id,
            name,
            text,
            line_starts,
        }
    }

    /// 1-based line and column (in bytes) for a byte offset.
    #[must_use]
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let index = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let line = u32::try_from(index).unwrap_or(u32::MAX) + 1;
        let col = offset.saturating_sub(self.line_starts[index]) + 1;
        (line, col)
    }

    #[must_use]
    pub fn line_text(&self, line_0: usize) -> &str {
        let Some(&start) = self.line_starts.get(line_0) else {
            return "";
        };
        let end = self
            .line_starts
            .get(line_0 + 1)
            .copied()
            .unwrap_or(u32::try_from(self.text.len()).unwrap_or(u32::MAX));
        let start = usize::try_from(start).unwrap_or(self.text.len());
        let end = usize::try_from(end).unwrap_or(self.text.len());
        self.text
            .get(start..end)
            .unwrap_or("")
            .trim_end_matches(['\r', '\n'])
    }

    /// Human rendering of a span: `path:line:col: message` plus a caret line.
    #[must_use]
    pub fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        let (line, col) = self.line_col(diagnostic.primary.start);
        let mut out = format!(
            "{}:{}:{}: {}: {}",
            self.name, line, col, diagnostic.code, diagnostic.message
        );
        let line_text = self.line_text(usize::try_from(line).unwrap_or(0).saturating_sub(1));
        if !line_text.is_empty() {
            out.push('\n');
            out.push_str("  ");
            out.push_str(line_text);
            out.push('\n');
            out.push_str("  ");
            for _ in 1..col {
                out.push(' ');
            }
            out.push('^');
            let width = usize::try_from(diagnostic.primary.len())
                .unwrap_or(0)
                .max(1);
            for _ in 1..width.min(80) {
                out.push('~');
            }
        }
        for (related, note) in &diagnostic.related {
            let (rline, rcol) = self.line_col(related.start);
            let suffix = format!(" (at {rline}:{rcol})");
            out.push_str("\n  = note: ");
            out.push_str(note);
            out.push_str(&suffix);
        }
        if let Some(help) = &diagnostic.help {
            out.push_str("\n  = help: ");
            out.push_str(help);
        }
        out
    }
}

/// Session store of loaded sources, allocating `FileId`s.
#[derive(Clone, Debug, Default)]
pub struct SourceStore {
    files: Vec<SourceFile>,
    by_name: HashMap<String, FileId>,
}

impl SourceStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    /// Add a source by name (path) and text. Returns the new file id.
    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        let name = name.into();
        if let Some(&id) = self.by_name.get(&name) {
            return id;
        }
        let id = FileId(u32::try_from(self.files.len()).unwrap_or(u32::MAX));
        self.files
            .push(SourceFile::build(text.into(), id, name.clone()));
        self.by_name.insert(name, id);
        id
    }

    /// Load from disk. Returns `Err` with the OS error string.
    pub fn load_path(&mut self, path: impl AsRef<Path>) -> Result<FileId, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        Ok(self.add(path.display().to_string(), text))
    }

    #[must_use]
    pub fn get(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(usize::try_from(id.0).unwrap_or(usize::MAX))
    }

    #[must_use]
    pub fn file_name(&self, id: FileId) -> Option<&str> {
        self.get(id).map(|file| file.name.as_str())
    }

    #[must_use]
    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    /// Render a diagnostic with line/column information.
    #[must_use]
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        self.get(diagnostic.primary.file).map_or_else(
            || diagnostic.to_string(),
            |file| file.render_diagnostic(diagnostic),
        )
    }

    /// Render all diagnostics separated by newlines.
    #[must_use]
    pub fn render_all(&self, diagnostics: &[Diagnostic]) -> String {
        diagnostics
            .iter()
            .map(|d| self.render(d))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

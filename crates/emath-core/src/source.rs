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
    fn build(text: String, id: FileId, name: String) -> Result<Self, String> {
        // Line offsets are `u32`; refuse sources whose byte length cannot be
        // addressed without collapsing distinct offsets to `u32::MAX`.
        let _len = u32::try_from(text.len())
            .map_err(|_| format!("source `{name}` length {} exceeds u32::MAX", text.len()))?;
        let mut line_starts = Vec::new();
        line_starts.push(0);
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                let start = u32::try_from(index + 1).map_err(|_| {
                    format!("source `{name}` line offset {} exceeds u32::MAX", index + 1)
                })?;
                line_starts.push(start);
            }
        }
        Ok(Self {
            id,
            name,
            text,
            line_starts,
        })
    }

    /// 1-based line and column (in bytes) for a byte offset.
    #[must_use]
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let index = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        // `line_starts` always contains at least `[0]`; index is in range.
        let line = u32::try_from(index)
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        let col = offset
            .saturating_sub(self.line_starts[index])
            .saturating_add(1);
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

    /// Spaces before the caret: Unicode scalar count within the line so a
    /// multi-byte prefix does not shift the caret right of the span.
    fn caret_indent(line_text: &str, byte_col_1: u32) -> usize {
        let byte_off = usize::try_from(byte_col_1.saturating_sub(1)).unwrap_or(0);
        let mut boundary = byte_off.min(line_text.len());
        while boundary > 0 && !line_text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        line_text[..boundary].chars().count()
    }

    /// Flatten ASCII controls in diagnostic prose so a crafted message cannot
    /// inject extra `path:line:col:` lines into human/tooling output.
    fn flatten_prose(text: &str) -> String {
        text.chars()
            .map(|ch| match ch {
                '\n' | '\r' => ' ',
                ch if (ch as u32) < 0x20 || ch == '\u{7f}' => ' ',
                ch => ch,
            })
            .collect()
    }

    /// Human rendering of a span: `path:line:col: message` plus a caret line.
    #[must_use]
    pub fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        let (line, col) = self.line_col(diagnostic.primary.start);
        let mut out = format!(
            "{}:{}:{}: {}: {}",
            Self::flatten_prose(&self.name),
            line,
            col,
            diagnostic.code,
            Self::flatten_prose(&diagnostic.message)
        );
        let line_text = self.line_text(usize::try_from(line).unwrap_or(0).saturating_sub(1));
        if !line_text.is_empty() {
            out.push('\n');
            out.push_str("  ");
            out.push_str(line_text);
            out.push('\n');
            out.push_str("  ");
            for _ in 0..Self::caret_indent(line_text, col) {
                out.push(' ');
            }
            out.push('^');
            let end_col = self.line_col(diagnostic.primary.end).1;
            let width = Self::caret_indent(line_text, end_col)
                .saturating_sub(Self::caret_indent(line_text, col))
                .max(1);
            for _ in 1..width.min(80) {
                out.push('~');
            }
        }
        for (related, note) in &diagnostic.related {
            let (rline, rcol) = self.line_col(related.start);
            let suffix = format!(" (at {rline}:{rcol})");
            out.push_str("\n  = note: ");
            out.push_str(&Self::flatten_prose(note));
            out.push_str(&suffix);
        }
        if let Some(help) = &diagnostic.help {
            out.push_str("\n  = help: ");
            out.push_str(&Self::flatten_prose(help));
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
    ///
    /// Fails when the store already holds `u32::MAX` files or when `text`
    /// cannot be addressed with `u32` byte offsets.
    pub fn try_add(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<FileId, String> {
        let name = name.into();
        if let Some(&id) = self.by_name.get(&name) {
            return Ok(id);
        }
        let id = FileId(
            u32::try_from(self.files.len())
                .map_err(|_| "source store file count exceeds u32::MAX".to_string())?,
        );
        self.files
            .push(SourceFile::build(text.into(), id, name.clone())?);
        self.by_name.insert(name, id);
        Ok(id)
    }

    /// Add a source by name (path) and text. Returns the new file id.
    ///
    /// # Panics
    ///
    /// Panics if [`Self::try_add`] would fail (oversized text or file-id
    /// exhaustion). Prefer [`Self::try_add`] at fallible boundaries.
    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        self.try_add(name, text)
            .expect("source store add refused oversized text or file-id overflow")
    }

    /// Load from disk. Returns `Err` with the OS error string.
    pub fn load_path(&mut self, path: impl AsRef<Path>) -> Result<FileId, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        self.try_add(path.display().to_string(), text)
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

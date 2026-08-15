//! Byte spans over source files.

use crate::id::FileId;

/// Half-open byte range `[start, end)` in the file identified by `file`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    #[must_use]
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Smallest span covering both inputs (`file` must match).
    #[must_use]
    pub fn cover(self, other: Self) -> Self {
        debug_assert_eq!(self.file, other.file);
        Self {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    #[must_use]
    pub fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }

    #[must_use]
    pub fn cursor(file: FileId, offset: u32) -> Self {
        Self::new(file, offset, offset)
    }
}

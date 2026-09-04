//! Compiled semantic image: a field pack compiles into a
//! compact, deterministic image — cells, bytecode, worlds, docs offsets,
//! identities — organized in independently loadable partitions under a
//! content id, with a lock recording prelude/packs/images/toolchain.
//!
//! The image is DATA (canonical text partitions), never a tree of
//! generated .rs files as source of truth, and it is not the user model.
//! Every partition carries a content id; the loader validates each
//! partition independently and a corrupt page refuses typed — never a
//! silent load, never partial authority.

use std::collections::BTreeMap;
use std::fmt;

use emath_core::fnv1a64_bytes;

use crate::term_compile::CompiledCell;

/// Canonical schema id for the compiled semantic image.
pub const SEMANTIC_IMAGE_SCHEMA: &str = "emath.semantic-image";
/// Image schema version. Bump on any change to the canonical encoding.
pub const SEMANTIC_IMAGE_VERSION: u32 = 1;

/// One world evidence record, as image DATA (decoded from any World ABI
/// producer; the strict image never imports the custom-lane crates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageWorld {
    /// Stable world token.
    pub world: String,
    /// Origin class (`seed` / `user-defined` / `synthesized`).
    pub origin: String,
    /// Claimed laws.
    pub laws: Vec<String>,
}

/// Closed partition kinds. A new partition class is a schema decision
/// recorded here, never a silent extra page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PartitionKind {
    /// Admitted cells (identity + schema records).
    Cells,
    /// Compiled cell bytecode (the term compiler's generic EMIR).
    Bytecode,
    /// World evidence records (World ABI producers).
    Worlds,
    /// Docs offsets, keyed by cell name.
    Docs,
    /// The lock: prelude/packs/images/toolchain identities.
    Lock,
}

impl PartitionKind {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cells => "cells",
            Self::Bytecode => "worlds.bytecode",
            Self::Worlds => "worlds",
            Self::Docs => "docs",
            Self::Lock => "lock",
        }
    }

    /// Parses the stable token; anything else refuses.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "cells" => Some(Self::Cells),
            "worlds.bytecode" => Some(Self::Bytecode),
            "worlds" => Some(Self::Worlds),
            "docs" => Some(Self::Docs),
            "lock" => Some(Self::Lock),
            _ => None,
        }
    }
}

/// One independently loadable image page: a named partition whose body is
/// canonical text stamped with a content id. A flipped byte after
/// stamping is a typed [`ImageRefusal::CorruptPartition`], never a
/// silent load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImagePartition {
    /// Stable page name (the partition kind token).
    pub name: String,
    /// Closed partition kind.
    pub kind: PartitionKind,
    /// Content id: `fnv1a64:<hex>` over the body bytes.
    pub content_id: String,
    /// Canonical text payload.
    pub body: String,
}

impl ImagePartition {
    /// Stamps a partition from its body: the content id binds the bytes.
    #[must_use]
    pub fn stamp(name: &str, kind: PartitionKind, body: &str) -> Self {
        Self {
            name: name.to_string(),
            kind,
            content_id: format!("fnv1a64:{:016x}", fnv1a64_bytes(body.as_bytes())),
            body: body.to_string(),
        }
    }

    /// Independently loadable: validates the name and re-derives the
    /// content id from the body.
    pub fn validate(&self) -> Result<(), ImageRefusal> {
        if self.name.is_empty() || self.body.is_empty() {
            return Err(ImageRefusal::MalformedPartition {
                name: self.name.clone(),
            });
        }
        let expected = format!("fnv1a64:{:016x}", fnv1a64_bytes(self.body.as_bytes()));
        if self.content_id != expected {
            return Err(ImageRefusal::CorruptPartition {
                name: self.name.clone(),
            });
        }
        Ok(())
    }
}

/// Typed refusal of a corrupt or malformed image page. Closed set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageRefusal {
    /// `E-IMAGE-001` — a partition's body no longer matches its stamped
    /// content id: a corrupt page.
    CorruptPartition {
        /// The partition that refused.
        name: String,
    },
    /// `E-IMAGE-002` — a partition with no name or no page body.
    MalformedPartition {
        /// The partition that refused.
        name: String,
    },
}

impl ImageRefusal {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::CorruptPartition { .. } => "E-IMAGE-001",
            Self::MalformedPartition { .. } => "E-IMAGE-002",
        }
    }
}

impl fmt::Display for ImageRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CorruptPartition { name } => write!(
                formatter,
                "semantic image partition `{name}` is corrupt: content id does \
                 not match its page (E-IMAGE-001)"
            ),
            Self::MalformedPartition { name } => write!(
                formatter,
                "semantic image partition `{name}` is malformed: empty name or \
                 empty page (E-IMAGE-002)"
            ),
        }
    }
}

impl std::error::Error for ImageRefusal {}

/// The lock: identities the image was built against. A lock mismatch
/// (prelude/pack/toolchain drift) is a load decision, recorded as data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageLock {
    /// Prelude identities (`std.prelude.core@1.0.0`).
    pub prelude: Vec<String>,
    /// Source pack identities (`name@version`).
    pub packs: Vec<String>,
    /// Other image ids this image depends on.
    pub images: Vec<String>,
    /// Toolchain identity.
    pub toolchain: String,
}

impl ImageLock {
    fn canonical(&self) -> String {
        // One line per identity section; each header is always present so
        // the lock shape is stable even when a list is empty.
        let join = |entries: &[String]| entries.join(";");
        let mut out = String::new();
        out.push_str("prelude:");
        out.push_str(&join(&self.prelude));
        out.push('\n');
        out.push_str("packs:");
        out.push_str(&join(&self.packs));
        out.push('\n');
        out.push_str("images:");
        out.push_str(&join(&self.images));
        out.push('\n');
        out.push_str("toolchain:");
        out.push_str(&self.toolchain);
        out.push('\n');
        out
    }
}

/// The compiled semantic image: partitions sorted by name, stamped with
/// content ids, under a deterministic image id. Built FROM cells (the
/// term compiler's output); the bytecode partition carries the compiled
/// cell bytecode — not generated Rust source, and not the user model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticImage {
    /// The pack the image was built from.
    pub pack_name: String,
    /// Partitions, sorted by name.
    pub partitions: Vec<ImagePartition>,
    /// Content id: `fnv1a64:<hex>` over the canonical encoding.
    pub image_id: String,
}

impl SemanticImage {
    /// Builds the deterministic image for a pack: cells + worlds + docs
    /// + lock, each partition canonical and stamped. Identical inputs
    /// rebuild the identical image (deterministic, replayable).
    pub fn build(
        pack_name: &str,
        cells: &[CompiledCell],
        worlds: &[ImageWorld],
        docs: &BTreeMap<String, String>,
        lock: ImageLock,
    ) -> Result<Self, ImageRefusal> {
        if pack_name.is_empty() {
            return Err(ImageRefusal::MalformedPartition {
                name: "<pack>".to_string(),
            });
        }
        let mut partitions = Vec::new();
        // Cells: identity + schema records (name, class, params, guards).
        let mut cells_body = String::new();
        let mut cells_sorted = cells.iter().collect::<Vec<_>>();
        cells_sorted.sort_by(|a, b| a.capability.cmp(&b.capability));
        for cell in cells_sorted.iter().copied() {
            cells_body.push_str("cell:");
            cells_body.push_str(&cell.capability);
            cells_body.push_str(" class=params:");
            for (name, shape) in &cell.params {
                cells_body.push_str(name);
                cells_body.push(':');
                cells_body.push_str(shape.as_str());
                cells_body.push(';');
            }
            cells_body.push_str(" guards=");
            cells_body.push_str(&cell.guards.len().to_string());
            cells_body.push('\n');
        }
        partitions.push(ImagePartition::stamp(
            PartitionKind::Cells.as_str(),
            PartitionKind::Cells,
            &cells_body,
        ));
        // Bytecode: each cell's compiled program, byte-deterministic SSA.
        let mut bytecode_body = String::new();
        for cell in cells_sorted.iter().copied() {
            bytecode_body.push_str("cell:");
            bytecode_body.push_str(&cell.capability);
            bytecode_body.push('\n');
            bytecode_body.push_str(&cell.program.print());
        }
        partitions.push(ImagePartition::stamp(
            PartitionKind::Bytecode.as_str(),
            PartitionKind::Bytecode,
            &bytecode_body,
        ));
        // Worlds: evidence records, sorted.
        let mut worlds_sorted = worlds.to_vec();
        worlds_sorted.sort_by(|a, b| a.world.cmp(&b.world));
        let mut worlds_body = String::new();
        for world in worlds_sorted {
            worlds_body.push_str("world:");
            worlds_body.push_str(&world.world);
            worlds_body.push_str(" origin=");
            worlds_body.push_str(&world.origin);
            worlds_body.push_str(" laws=");
            for law in &world.laws {
                worlds_body.push_str(law);
                worlds_body.push(';');
            }
            worlds_body.push('\n');
        }
        partitions.push(ImagePartition::stamp(
            PartitionKind::Worlds.as_str(),
            PartitionKind::Worlds,
            &worlds_body,
        ));
        // Docs: offsets keyed by cell name (BTreeMap = sorted).
        let mut docs_body = String::new();
        for (name, text) in docs {
            docs_body.push_str("doc:");
            docs_body.push_str(name);
            docs_body.push_str(" bytes=");
            docs_body.push_str(&text.len().to_string());
            docs_body.push('\n');
        }
        partitions.push(ImagePartition::stamp(
            PartitionKind::Docs.as_str(),
            PartitionKind::Docs,
            &docs_body,
        ));
        // Lock.
        partitions.push(ImagePartition::stamp(
            PartitionKind::Lock.as_str(),
            PartitionKind::Lock,
            &lock.canonical(),
        ));
        partitions.sort_by(|a, b| a.name.cmp(&b.name));
        let image = Self {
            pack_name: pack_name.to_string(),
            partitions,
            image_id: String::new(),
        };
        let mut canonical = String::new();
        canonical.push_str(SEMANTIC_IMAGE_SCHEMA);
        canonical.push('\n');
        canonical.push_str(&SEMANTIC_IMAGE_VERSION.to_string());
        canonical.push('\n');
        canonical.push_str(pack_name);
        canonical.push('\n');
        for partition in &image.partitions {
            canonical.push_str(&partition.name);
            canonical.push(':');
            canonical.push_str(&partition.content_id);
            canonical.push('\n');
        }
        Ok(Self {
            image_id: format!("fnv1a64:{:016x}", fnv1a64_bytes(canonical.as_bytes())),
            ..image
        })
    }

    /// Validates every partition independently; the first corrupt page
    /// refuses typed (the loader never returns a partial image).
    pub fn validate_partitions(&self) -> Result<(), ImageRefusal> {
        for partition in &self.partitions {
            partition.validate()?;
        }
        Ok(())
    }

    /// Loads one partition's body by page name (independent read;
    /// `None` = not a page of this image).
    #[must_use]
    pub fn load(&self, name: &str) -> Option<&str> {
        self.partitions
            .iter()
            .find(|partition| partition.name == name)
            .map(|partition| partition.body.as_str())
    }

    /// Canonical encoding (schema, version, pack, stamped partition ids).
    #[must_use]
    pub fn to_canonical(&self) -> String {
        let mut out = String::new();
        out.push_str(SEMANTIC_IMAGE_SCHEMA);
        out.push('\n');
        out.push_str(&SEMANTIC_IMAGE_VERSION.to_string());
        out.push('\n');
        out.push_str(&self.pack_name);
        out.push('\n');
        out.push_str(&self.image_id);
        out.push('\n');
        for partition in &self.partitions {
            out.push_str(&partition.name);
            out.push(':');
            out.push_str(&partition.content_id);
            out.push('\n');
        }
        out
    }
}

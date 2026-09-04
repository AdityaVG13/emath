//! Reachable-closure analysis and semantic tree shaking.
//!
//! Generated artifacts must not contain unused mathematics. The
//! reachable closure over a [`SemanticImage`] starts from the
//! pinned entry set and keeps every cell the artifact can reach;
//! shaking drops the UNREACHABLE cells' bytecode from the ARTIFACT —
//! never from source ("do not delete source cells": the `cells` page's
//! schema records stay; only the bytecode page shrinks).
//!
//! Edges: the bytecode page is per-cell sections (`cell:<identity>`
//! headers followed by the compiled program); an
//! `apply-capability name=<identity>` reference is the closure's ONLY
//! internal edge type. Entries are the artifact's roots; everything
//! reachable from an entry is REQUIRED and cannot be demoted — naming a
//! required dependency as an entry too refuses typed (`E-SHAKE-002`,
//! the negative seed's silent-success: a smaller-but-broken artifact).
//! Shaking an unknown identity refuses typed (`E-SHAKE-001`) — never a
//! silent no-op.
//!
//! The shaken image keeps the determinism law (sorted, stamped,
//! self-validating): its id changes because its content changed, and a
//! no-op shake rebuilds the identical image. An empty closure ships no
//! bytecode page at all (an empty page cannot validate under
//! `E-IMAGE-002`; the lock/worlds/docs partitions are the artifact's
//! remaining identity).

use std::collections::BTreeSet;

use crate::image::{ImagePartition, PartitionKind, SemanticImage};

/// The bytecode partition's stable page name.
const BYTECODE_PAGE: &str = "worlds.bytecode";

/// Shaking refusal. Closed set; every variant names the identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShakeError {
    /// `E-SHAKE-001` — a shake entry the image does not contain (never
    /// a silent no-op that pretends to shake).
    UnknownCell {
        /// The unknown capability identity.
        capability: String,
    },
    /// `E-SHAKE-002` — a REQUIRED dependency (reachable from another
    /// entry) cannot be demoted to a root; the smaller-but-broken
    /// artifact is refused.
    RequiredDependency {
        /// The required capability the request tried to re-root.
        capability: String,
    },
}

impl ShakeError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownCell { .. } => "E-SHAKE-001",
            Self::RequiredDependency { .. } => "E-SHAKE-002",
        }
    }
}

impl std::fmt::Display for ShakeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCell { capability } => write!(
                formatter,
                "{code}: cannot shake `{capability}` — not a cell of this image",
                code = self.code()
            ),
            Self::RequiredDependency { capability } => write!(
                formatter,
                "{code}: `{capability}` is reachable from another entry — \
                 required dependencies cannot be shaken out",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for ShakeError {}

/// The tree-shaken artifact plus its closure report. The shaken image
/// keeps the invariants (sorted partitions, stamped ids, one
/// fnv1a64 image id over the canonical encoding).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShakenImage {
    /// The shaken artifact (unreachable bytecode dropped).
    pub shaken: SemanticImage,
    /// Entry identities the closure started from (in request order).
    entries: Vec<String>,
    /// Cell identities the closure KEPT (reachable), sorted.
    kept: Vec<String>,
}

impl ShakenImage {
    /// How many entries the closure started from.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Whether a cell identity survived the shake (reachable).
    #[must_use]
    pub fn is_kept(&self, capability: &str) -> bool {
        self.kept.iter().any(|identity| identity == capability)
    }

    /// The kept (reachable) identities, sorted.
    #[must_use]
    pub fn kept(&self) -> &[String] {
        &self.kept
    }
}

/// The image's cell identities from the `cells` page (`cell:<identity>
/// class=...` records, in the builder's sorted order).
fn image_cells(image: &SemanticImage) -> Vec<String> {
    let Some(cells_page) = image.load("cells") else {
        return Vec::new();
    };
    cells_page
        .lines()
        .filter_map(|line| line.strip_prefix("cell:"))
        .map(|record| match record.find(" class=") {
            Some(end) => record[..end].to_string(),
            None => record.to_string(),
        })
        .collect()
}

/// The cross-cell edges: every `apply-capability name=<identity>`
/// payload inside the bytecode page, paired with the OWNING cell (the
/// `cell:<identity>` section header that precedes it). Deterministic:
/// collected into a sorted set.
fn image_edges(image: &SemanticImage) -> BTreeSet<(String, String)> {
    let mut edges = BTreeSet::new();
    let Some(bytecode) = image.load(BYTECODE_PAGE) else {
        return edges;
    };
    let mut owner = String::new();
    for line in bytecode.lines() {
        if let Some(identity) = line.strip_prefix("cell:") {
            owner = identity.to_string();
        } else if let Some((_head, tail)) = line.split_once("apply-capability ") {
            for token in tail.split_whitespace() {
                if let Some(capability) = token.strip_prefix("name=") {
                    if !owner.is_empty() {
                        edges.insert((owner.clone(), capability.to_string()));
                    }
                }
            }
        }
    }
    edges
}

/// The reachable closure from `roots` over the image's edges (BFS to
/// fixpoint; the only edge type is the apply-capability reference).
fn closure_from(roots: &[&str], edges: &BTreeSet<(String, String)>) -> BTreeSet<String> {
    let mut reached: BTreeSet<String> = roots.iter().map(|root| (*root).to_string()).collect();
    loop {
        let mut grew = false;
        for (owner, target) in edges {
            if reached.contains(owner) && reached.insert(target.clone()) {
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    reached
}

/// Tree-shake an image: compute the reachable closure from `entries`
/// (the artifact's roots) over the bytecode page's apply-capability
/// edges, then rebuild the artifact with only the KEPT cells' bytecode
/// sections. The `cells` page (the source records) is never touched.
///
/// Refusals: an entry the image does not contain is
/// [`ShakeError::UnknownCell`]; an entry reachable from ANOTHER entry is
/// a required dependency being demoted — [`ShakeError::RequiredDependency`]
/// refuses the request as a whole (no partial artifact escapes).
pub fn shake_image(image: &SemanticImage, entries: &[&str]) -> Result<ShakenImage, ShakeError> {
    let cells = image_cells(image);
    for entry in entries {
        if !cells.iter().any(|identity| identity == entry) {
            return Err(ShakeError::UnknownCell {
                capability: (*entry).to_string(),
            });
        }
    }
    let edges = image_edges(image);
    for (index, entry) in entries.iter().enumerate() {
        let others: Vec<&str> = entries
            .iter()
            .enumerate()
            .filter_map(|(other, candidate)| (other != index).then_some(*candidate))
            .collect();
        if !others.is_empty() && closure_from(&others, &edges).iter().any(|id| id == entry) {
            return Err(ShakeError::RequiredDependency {
                capability: (*entry).to_string(),
            });
        }
    }
    let kept: Vec<String> = closure_from(entries, &edges).into_iter().collect();
    Ok(ShakenImage {
        shaken: rebuild(image, &kept),
        entries: entries.iter().map(|entry| (*entry).to_string()).collect(),
        kept,
    })
}

/// The bytecode page's per-cell sections: `(identity, body-lines)`
/// pairs in page order, each section's lines newline-terminated.
fn bytecode_sections(bytecode: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut owner = String::new();
    for line in bytecode.lines() {
        if let Some(identity) = line.strip_prefix("cell:") {
            sections.push((identity.to_string(), String::new()));
            owner = identity.to_string();
        } else if !owner.is_empty() {
            let section = &mut sections
                .last_mut()
                .expect("a section exists once an owner is set")
                .1;
            section.push_str(line);
            section.push('\n');
        }
    }
    sections
}

/// Rebuild the artifact with only the kept cells' bytecode sections;
/// every other partition passes through unchanged (the cells page is
/// the source records — "do not delete source cells"). The image id is
/// re-derived by the canonical recipe, so a content change is
/// always a new id and an identical shake is the identical image.
fn rebuild(image: &SemanticImage, kept: &[String]) -> SemanticImage {
    let mut partitions: Vec<ImagePartition> = Vec::new();
    for partition in &image.partitions {
        if partition.name == BYTECODE_PAGE {
            if kept.is_empty() {
                // No reachable bytecode: the page is not shipped (an
                // empty page would refuse E-IMAGE-002 on load).
                continue;
            }
            let mut body = String::new();
            let sections = bytecode_sections(&partition.body);
            for (identity, section) in &sections {
                if kept.iter().any(|kept_id| kept_id == identity) {
                    body.push_str("cell:");
                    body.push_str(identity);
                    body.push('\n');
                    body.push_str(section);
                }
            }
            partitions.push(ImagePartition::stamp(
                &partition.name,
                PartitionKind::Bytecode,
                &body,
            ));
        } else {
            partitions.push(partition.clone());
        }
    }
    let mut canonical = String::new();
    canonical.push_str(crate::image::SEMANTIC_IMAGE_SCHEMA);
    canonical.push('\n');
    canonical.push_str(&crate::image::SEMANTIC_IMAGE_VERSION.to_string());
    canonical.push('\n');
    canonical.push_str(&image.pack_name);
    canonical.push('\n');
    for partition in &partitions {
        canonical.push_str(&partition.name);
        canonical.push(':');
        canonical.push_str(&partition.content_id);
        canonical.push('\n');
    }
    SemanticImage {
        pack_name: image.pack_name.clone(),
        partitions,
        image_id: format!(
            "fnv1a64:{:016x}",
            emath_core::fnv1a64_bytes(canonical.as_bytes())
        ),
    }
}

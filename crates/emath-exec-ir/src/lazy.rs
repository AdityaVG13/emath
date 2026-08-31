//! Lazy image loading, prelude-only startup, optional chunks (fjxh.10).
//!
//! Startup must not compile (or load) all installed fields. A lazy
//! session boots with the NUCLEUS + the PRELUDE INDEX (each installed
//! [`SemanticImage`]'s lock page) and loads reachable packs' pages on
//! demand at file compile. The initialization receipt names exactly the
//! loaded pages — an UNUSED pack's pages never load, and any attempt to
//! serve one refuses typed (`E-LAZY-001`, the negative seed's
//! silent-success: an eager loader wearing a lazy label). Unknown packs
//! refuse typed (`E-LAZY-002`) at boot (custom profile) and at compile.
//! The packs that stay unloaded are the artifact's optional WASM
//! chunks, named deterministically (sorted).
//!
//! This module models the loader's CONTRACT over in-memory images: the
//! receipt + refusals are the law, so an eager implementation cannot
//! pass the suite (the receipt would name pages the profile never
//! admitted, and unused-pack access would silently succeed).

use std::collections::BTreeSet;

use crate::image::SemanticImage;

/// The prelude index page (each installed image's lock partition).
const PRELUDE_INDEX_PAGE: &str = "lock";
/// The nucleus root: the always-loaded reference nucleus (this crate).
const NUCLEUS_ROOT: &str = "nucleus";

/// Startup admission profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadProfile {
    /// Nucleus + prelude index only; every field page loads on demand.
    Minimal,
    /// Nucleus + prelude index + every installed pack's pages.
    Standard,
    /// Nucleus + prelude index + exactly the named packs' pages.
    /// Naming an uninstalled pack refuses at boot.
    Custom(Vec<String>),
}

/// Lazy-loading refusal. Closed set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LazyError {
    /// `E-LAZY-001` — a page was requested from a pack the session
    /// never loaded (unused-pack access is detected, never served).
    UnloadedPackAccess {
        /// The unloaded pack.
        pack: String,
        /// The requested page.
        page: String,
    },
    /// `E-LAZY-002` — a pack nobody installed was named (at boot in a
    /// custom profile, or at file compile).
    UnknownPack {
        /// The unknown pack.
        pack: String,
    },
}

impl LazyError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnloadedPackAccess { .. } => "E-LAZY-001",
            Self::UnknownPack { .. } => "E-LAZY-002",
        }
    }
}

impl std::fmt::Display for LazyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnloadedPackAccess { pack, page } => write!(
                formatter,
                "{code}: `{pack}/{page}` was never loaded — unused pack \
                 pages never load on a lazy session",
                code = self.code()
            ),
            Self::UnknownPack { pack } => write!(
                formatter,
                "{code}: `{pack}` is not installed",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for LazyError {}

/// The initialization receipt: exactly the pages the session loaded,
/// sorted (`nucleus` root + `<pack>/<page>` tokens). The receipt is the
/// startup proof — an eager loader cannot fake it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializationReceipt {
    loaded_pages: Vec<String>,
}

impl InitializationReceipt {
    /// Whether the receipt names `<pack>/<page>`.
    #[must_use]
    pub fn names(&self, pack: &str, page: &str) -> bool {
        self.loaded_pages
            .iter()
            .any(|token| token == &format!("{pack}/{page}"))
    }

    /// Whether the receipt names a root (`nucleus`).
    #[must_use]
    pub fn names_root(&self, root: &str) -> bool {
        self.loaded_pages.iter().any(|token| token == root)
    }

    /// The loaded page tokens, sorted.
    #[must_use]
    pub fn loaded_pages(&self) -> &[String] {
        &self.loaded_pages
    }

    fn from_loaded(loaded: &BTreeSet<String>) -> Self {
        Self {
            loaded_pages: loaded.iter().cloned().collect(),
        }
    }
}

/// A lazy session over the installed images: boots with nucleus +
/// prelude index (per the profile), loads reachable packs on demand,
/// serves only loaded pages, and reports the initialization receipt.
#[derive(Clone, Debug)]
pub struct LazySession {
    installed: Vec<SemanticImage>,
    loaded: BTreeSet<String>,
}

impl LazySession {
    /// Boots a session: the nucleus root + each installed image's
    /// prelude index (lock) page, plus the profile's admitted packs'
    /// pages. A custom profile naming an uninstalled pack refuses
    /// typed before anything loads.
    pub fn boot(installed: &[SemanticImage], profile: LoadProfile) -> Result<Self, LazyError> {
        let installed = installed.to_vec();
        let mut loaded = BTreeSet::new();
        loaded.insert(NUCLEUS_ROOT.to_string());
        for image in &installed {
            loaded.insert(format!("{}/{}", image.pack_name, PRELUDE_INDEX_PAGE));
        }
        let admitted: Vec<String> = match profile {
            LoadProfile::Minimal => Vec::new(),
            LoadProfile::Standard => installed
                .iter()
                .map(|image| image.pack_name.clone())
                .collect(),
            LoadProfile::Custom(packs) => {
                for pack in &packs {
                    if !installed.iter().any(|image| &image.pack_name == pack) {
                        return Err(LazyError::UnknownPack { pack: pack.clone() });
                    }
                }
                packs
            }
        };
        let mut session = Self { installed, loaded };
        for pack in &admitted {
            session.load_pack_pages(pack);
        }
        Ok(session)
    }

    /// The receipt of everything loaded so far (deterministic, sorted).
    #[must_use]
    pub fn receipt(&self) -> InitializationReceipt {
        InitializationReceipt::from_loaded(&self.loaded)
    }

    /// Serves a page body from a LOADED pack page; a page from a pack
    /// the session never loaded refuses typed (`E-LAZY-001`) — never a
    /// silent eager fallback.
    pub fn page(&self, pack: &str, page: &str) -> Result<&str, LazyError> {
        if self.loaded.contains(&format!("{pack}/{page}")) {
            let image = self
                .installed
                .iter()
                .find(|image| image.pack_name == pack)
                .expect("a loaded pack is installed");
            let body = image.load(page).expect("a loaded page exists in its image");
            Ok(body)
        } else {
            Err(LazyError::UnloadedPackAccess {
                pack: pack.to_string(),
                page: page.to_string(),
            })
        }
    }

    /// File compile: loads exactly the named packs' pages (the file's
    /// reachable packs) and returns the post-load receipt. Unknown
    /// packs refuse typed; nothing loads on refusal.
    pub fn load_for_compile(
        &mut self,
        file_packs: &[&str],
    ) -> Result<InitializationReceipt, LazyError> {
        for pack in file_packs {
            if !self.installed.iter().any(|image| &image.pack_name == pack) {
                return Err(LazyError::UnknownPack {
                    pack: (*pack).to_string(),
                });
            }
        }
        for pack in file_packs {
            self.load_pack_pages(pack);
        }
        Ok(self.receipt())
    }

    /// Admits every page of one installed pack.
    fn load_pack_pages(&mut self, pack: &str) {
        let pages: Vec<String> = self
            .installed
            .iter()
            .find(|image| image.pack_name == pack)
            .map(|image| {
                image
                    .partitions
                    .iter()
                    .map(|partition| partition.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        for page in pages {
            self.loaded.insert(format!("{pack}/{page}"));
        }
    }
}

/// The installed packs whose field pages were never loaded: the
/// artifact's optional WASM chunks, sorted. A pack whose only loaded
/// page is its prelude index (lock) is still optional; a pack with any
/// field page loaded is not.
#[must_use]
pub fn optional_chunks(
    installed: &[SemanticImage],
    receipt: &InitializationReceipt,
) -> Vec<String> {
    let mut chunks: Vec<String> = installed
        .iter()
        .filter(|image| {
            image.partitions.iter().all(|partition| {
                partition.name == PRELUDE_INDEX_PAGE
                    || !receipt.names(&image.pack_name, &partition.name)
            })
        })
        .map(|image| image.pack_name.clone())
        .collect();
    chunks.sort();
    chunks
}

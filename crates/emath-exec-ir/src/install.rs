//! Field-pack install tooling: add a toy pack, `use` it, no
//! core branches.
//!
//! This module is the TOOLING half of the field-pack capstone — the
//! language kind (`emath field_pack`) is admission; here
//! an admitted [`emath_ir::FieldPackEntry`] becomes an INSTALLED
//! artifact:
//!
//! 1. **Layout** is a fixed directory set ([`LAYOUT_DIRS`], the
//!    fixed `pack.emath.toml` + `src/ worlds/ methods/ examples/
//!    providers/ migrations/` shape). A directory outside the set —
//!    e.g. a `keywords/` injection — refuses typed; packs cannot add
//!    parser surface through layout.
//! 2. **Install** resolves the pack's declared exports against the
//!    EXISTING std cell registry (exact canonical match first, then a
//!    unique leaf match) and compiles the resolved cells into the
//!    [`SemanticImage`] — the same data-driven builder, no new core
//!    branches, no compiler rebuild. Install never fabricates: an
//!    export nobody provides refuses typed, and a pack with no
//!    exportable cells has nothing to install (the image law requires
//!    non-empty pages).
//! 3. **Use** — `use <package>.<pack>` resolves against the installed
//!    pack registry ([`PackRegistry::resolve_use`]); an uninstalled
//!    path refuses typed. This is the data-level form of the language
//!    `use` admission.
//!
//! Install consumes ONLY admitted pack data (`FieldPackEntry`): pack
//! source that injects parser keywords is refused at admission
//! (`E-SYN-101`) and never reaches this module.

use std::collections::HashMap;

use crate::image::{ImageLock, ImageWorld, SemanticImage};
use crate::term_compile::CompiledCell;

/// The closed field-pack layout (fixed directories).
pub const LAYOUT_DIRS: &[&str] = &[
    "src",
    "worlds",
    "methods",
    "examples",
    "providers",
    "migrations",
];

/// Install-tooling refusal. Closed set; every variant names the cause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackError {
    /// `E-PACK-001` — a layout directory outside the closed set.
    UnknownLayoutDir {
        /// The injected directory name.
        dir: String,
    },
    /// `E-PACK-002` — an export the registry does not provide (install
    /// never fabricates a cell).
    UnknownExport {
        /// The unresolvable export name.
        export: String,
    },
    /// `E-PACK-003` — a `use` path with no installed pack.
    UnknownPack {
        /// The dotted use path.
        use_path: String,
    },
    /// `E-PACK-004` — an export name matching several registry cells
    /// (not uniquely resolvable; packs must name cells precisely).
    AmbiguousExport {
        /// The ambiguous export name.
        export: String,
    },
    /// `E-PACK-005` — the pack exports no cells (a metadata-only pack
    /// has nothing installable: the image law requires non-empty pages).
    NothingToInstall {
        /// The pack declaration name.
        pack: String,
    },
}

impl PackError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownLayoutDir { .. } => "E-PACK-001",
            Self::UnknownExport { .. } => "E-PACK-002",
            Self::UnknownPack { .. } => "E-PACK-003",
            Self::AmbiguousExport { .. } => "E-PACK-004",
            Self::NothingToInstall { .. } => "E-PACK-005",
        }
    }
}

impl std::fmt::Display for PackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLayoutDir { dir } => write!(
                formatter,
                "{code}: layout directory `{dir}` is outside the closed field-pack \
                 layout ({dirs}) — packs cannot add parser surface through layout",
                code = self.code(),
                dirs = LAYOUT_DIRS.join(", ")
            ),
            Self::UnknownExport { export } => write!(
                formatter,
                "{code}: export `{export}` is not provided by the cell registry — \
                 install never fabricates a cell",
                code = self.code()
            ),
            Self::UnknownPack { use_path } => write!(
                formatter,
                "{code}: no installed pack for `use {use_path}`",
                code = self.code()
            ),
            Self::AmbiguousExport { export } => write!(
                formatter,
                "{code}: export `{export}` matches several registry cells — name \
                 the cell precisely",
                code = self.code()
            ),
            Self::NothingToInstall { pack } => write!(
                formatter,
                "{code}: pack `{pack}` exports no cells — nothing installable",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for PackError {}

/// Validates a pack layout against the closed directory set.
pub fn validate_layout(dirs: &[&str]) -> Result<(), PackError> {
    for dir in dirs {
        if !LAYOUT_DIRS.contains(&dir) {
            return Err(PackError::UnknownLayoutDir {
                dir: (*dir).to_string(),
            });
        }
    }
    Ok(())
}

/// One installed field pack: the admitted pack data plus its compiled
/// semantic image (the `.emlib` shape — self-validating image).
#[derive(Clone, Debug)]
pub struct InstalledPack {
    /// The `package <dotted>` identity the pack was declared under.
    pub package: Vec<String>,
    /// The pack declaration name.
    pub pack: String,
    /// The resolved canonical cell identities (source order).
    pub exports: Vec<String>,
    /// The compiled semantic image (cells + worlds + docs + lock).
    pub image: SemanticImage,
}

/// Resolves one export name against the registry: exact canonical match
/// first, then a unique leaf (`softmax` → `std.tensor.softmax`).
fn resolve_export(
    export: &str,
    registry: &HashMap<String, CompiledCell>,
) -> Result<String, PackError> {
    if registry.contains_key(export) {
        return Ok(export.to_string());
    }
    let matches: Vec<&String> = registry
        .keys()
        .filter(|canonical| {
            canonical
                .rsplit('.')
                .next()
                .is_some_and(|leaf| leaf == export)
        })
        .collect();
    match matches.as_slice() {
        [canonical] => Ok((*canonical).clone()),
        [] => Err(PackError::UnknownExport {
            export: export.to_string(),
        }),
        _ => Err(PackError::AmbiguousExport {
            export: export.to_string(),
        }),
    }
}

/// Installs an admitted pack: resolves the exports against the
/// registry, compiles the resolved cells into a deterministic semantic
/// image, and returns the installed artifact. Zero core branches: the
/// image builder and the registry are the same data paths every other
/// consumer uses.
pub fn install_pack(
    entry: &emath_ir::FieldPackEntry,
    package: &[String],
    registry: &HashMap<String, CompiledCell>,
) -> Result<InstalledPack, PackError> {
    install_composing_locked(entry, package, registry, Vec::new())
}

/// Installs an admitted pack whose lock records the EXISTING packages it
/// composes (the std::physics compose: composition, never forking).
/// `composed_packs` are lock identities (`name@version`) listed before
/// the pack's own identity, which is appended last and never duplicated.
pub fn install_pack_composing(
    entry: &emath_ir::FieldPackEntry,
    package: &[String],
    registry: &HashMap<String, CompiledCell>,
    composed_packs: &[String],
) -> Result<InstalledPack, PackError> {
    let mut packs: Vec<String> = composed_packs.to_vec();
    let own = format!("{}@0.1.0", entry.name);
    if !packs.contains(&own) {
        packs.push(own);
    }
    install_composing_locked(entry, package, registry, packs)
}

/// The shared install body: `packs` is the full lock list (composed
/// packages plus the pack's own identity, already deduplicated).
fn install_composing_locked(
    entry: &emath_ir::FieldPackEntry,
    package: &[String],
    registry: &HashMap<String, CompiledCell>,
    packs: Vec<String>,
) -> Result<InstalledPack, PackError> {
    validate_layout(&["src"])?;
    let mut exports: Vec<String> = Vec::with_capacity(entry.exports.len());
    for (export_kind, export) in &entry.exports {
        if export_kind != "cell" {
            // The registry slice installs CELLS; theory/method/world
            // exports are metadata records for their own epics — listed
            // verbatim, not compiled.
            continue;
        }
        exports.push(resolve_export(export, registry)?);
    }
    if exports.is_empty() {
        return Err(PackError::NothingToInstall {
            pack: entry.name.clone(),
        });
    }
    let registry_ref = registry;
    let cells: Vec<CompiledCell> = exports
        .iter()
        .map(|canonical| registry_ref.get(canonical).cloned())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| PackError::UnknownExport {
            export: "<registry drift>".to_string(),
        })?;
    let docs: std::collections::BTreeMap<String, String> = exports
        .iter()
        .map(|canonical| {
            (
                canonical.clone(),
                format!("exported by pack `{}`", entry.name),
            )
        })
        .collect();
    let image = SemanticImage::build(
        &entry.name,
        &cells,
        &[ImageWorld {
            world: "reference-vm".to_string(),
            origin: "field-pack-install".to_string(),
            laws: vec!["no-core-branches".to_string()],
        }],
        &docs,
        ImageLock {
            prelude: vec!["std.prelude.core@1.0.0".to_string()],
            packs,
            images: vec![],
            toolchain: "emath-toolchain@0.1.0".to_string(),
        },
    )
    .map_err(|_| PackError::NothingToInstall {
        pack: entry.name.clone(),
    })?;
    Ok(InstalledPack {
        package: package.to_vec(),
        pack: entry.name.clone(),
        exports,
        image,
    })
}

/// The installed pack registry: what a session has installed, and what
/// `use` resolves against.
#[derive(Clone, Debug, Default)]
pub struct PackRegistry {
    packs: Vec<InstalledPack>,
}

impl PackRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs a pack (append; identity is package + pack name).
    pub fn install(&mut self, pack: InstalledPack) {
        self.packs.push(pack);
    }

    /// The installed packs, in install order.
    #[must_use]
    pub fn packs(&self) -> &[InstalledPack] {
        &self.packs
    }

    /// Resolves `use <package>.<pack>` against the installed packs.
    pub fn resolve_use(&self, use_path: &[String]) -> Result<&InstalledPack, PackError> {
        let dotted = use_path.join(".");
        let (package, pack) = use_path
            .split_last()
            .map(|(pack, package)| (package, pack.as_str()))
            .ok_or_else(|| PackError::UnknownPack {
                use_path: dotted.clone(),
            })?;
        self.packs
            .iter()
            .find(|installed| installed.pack == pack && installed.package == package)
            .ok_or_else(|| PackError::UnknownPack { use_path: dotted })
    }
}

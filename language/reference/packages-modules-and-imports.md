# Packages, Modules, and Imports

## Manifest

Every package is rooted by `emath.toml`:

```toml
[package]
name = "demo"
version = "0.1.0"
edition = "2030"

[compile]
targets = ["rust-library"]
locked = true
```

The nearest ancestor manifest selects the package and language edition. Missing or unknown editions refuse. Manifest identity, dependencies, features, and numeric policy participate in package identity.

## Lock file

`emath.lock` records exact package sources, content identities, features, provider dependencies, and schema versions. It is deterministic and generated rather than hand-edited.

## Package declarations

A source file may declare its package:

```emath
package demo
```

Public identity is package path + module path + declaration name. Filesystem location alone does not define semantic identity.

## In-package imports

A file imports a loaded sibling module with a dotted path sharing its package prefix:

```emath
package demo
use demo.geometry
```

`demo.geometry` resolves to `geometry.emath` in the same compiler session. Its declarations and notation imports merge into the package. Duplicate declaration names are `E-NAME-022`; a sibling source that was not loaded is `E-PKG-050`. Module cycles refuse.

Elaboration follows a deterministic topological order. Reorganizing files does not change meaning when package, module, and declaration identities remain the same.

## Library imports

```emath
use physics::classical::{NewtonSecond, Hooke}
use sci::physics::notation::braket(convention = physics)
use std.kinds.capability
```

Selected symbols from embedded law packages resolve deterministically. Unknown symbols are `E-PKG-053`. The obsolete form `use physics::NewtonSecond` is `E-PKG-052`; write the complete package path.

Embedded law namespaces include:

```text
physics::classical       physics::relativity
cs::laws                 probability::laws
analysis::laws           number_theory::laws
optimization_control::laws
```

Wildcard imports cannot override an explicit or already-resolved name. Aliases and arbitrary remote registry resolution are not admitted.

## Imported kinds and notation

Declaration schemas and notation are opt-in package imports. A missing import never degrades to an unknown custom declaration or ambient glyph interpretation.

```emath
use std.kinds.method
use std.kinds.world
use sci::physics::notation::nabla
```

The import and its configuration are part of semantic identity.

## Field packs

A field pack exports existing capabilities and metadata:

```emath
package community

emath field_pack spectral_style:
    exports:
        cell softmax
        theory spectral
    metadata:
        description reference spectral pack
```

The public identity is `community.spectral_style`. Allowed sections are `exports:` and `metadata:`. Export commands are `cell`, `theory`, `method`, or `world` followed by a name. Unknown sections refuse, and pack source cannot introduce parser keywords.

Installation resolves every export from an existing registry and records the resolved package identities in the semantic-image lock. An unknown export is `E-PACK-002`; installation never fabricates a capability.

A composing pack, such as `std::physics`, lists every package it composes and deduplicates its own identity in the lock. Composition reuses existing cells and does not fork their semantics.

## Visibility and exports

Visibility is explicit. An `exports:` section selects public definitions or generated artifacts; declarations not exported remain package-internal. Re-export surfaces are defined by explicit `use` trees, not filesystem traversal.

## Editions

Supported editions define parser and deprecation policy without changing the meaning of already-admitted source. A package is always parsed under its manifest edition. Historical replay may select an older shipped edition explicitly.

Migration uses formatter-backed rewrites and must preserve semantic identity for presentation-only changes. Registered edition-major semantic corrections are a separate rule class: both old and corrected source must admit, their before/after `MeaningId` values must differ, and the receipt records the checked delta. If a site has several valid semantic rewrites, migration refuses with `E-MIG-AMBIGUOUS-SITE` and an ordered candidate list rather than guessing. An unsupported edition is a typed load error rather than a fallback to the newest grammar.

## Reproducibility

A reproducible package resolves all imports under its lock, uses a declared numeric profile and provider policy, and emits deterministic semantic identities. Unlocked or unresolved dependencies cannot silently enter a release artifact.

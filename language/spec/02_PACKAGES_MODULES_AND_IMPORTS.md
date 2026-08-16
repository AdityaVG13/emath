# Packages, Modules and Imports

## Package manifest

`emath.toml` declares:

```toml
[package]
name = "example"
version = "0.1.0"
edition = "2026"
license = "Apache-2.0"

[sources]
root = "src/lib.emath"

[dependencies]
core = { registry = "emath", version = "1" }

[compile]
targets = ["rust-library"]
locked = true
```

## Lock file

`emath.lock` records exact package source, content identity, features, provider/adaptor dependencies and schema versions. It is generated, deterministic and not hand-edited.

## Modules

One source file may contain one or more modules according to the manifest. Public identity is package + module path + declaration name. Module cycles are rejected in Phase 1; later mutually recursive modules require explicit semantics.

## Imports

```emath
use core::math::{Real, NonNegative}
use core::units::*
use provider::interval as ivl
```

Import resolution is deterministic under a lock. Wildcard imports cannot silently override an explicit or previously resolved name.

## Visibility

```text
public   available to package consumers
package  available within package
private  declaration-local/module-local according to construct
```

Generated Rust visibility is derived from emath visibility, not copied mechanically when doing so would let callers forge invariants.

## Feature configuration

Features select optional package capabilities. Feature resolution is part of the lock and artifact identity. A feature cannot silently change core source semantics without a declared semantic version impact.

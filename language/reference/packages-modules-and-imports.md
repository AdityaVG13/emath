# Chapter 3: Packages, Modules and Imports

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

## Notation Governance (N1–N5)

Notation declarations are scoped syntax aliases that map a glyph to an
operator path. They are package citizens: declared at the top level,
imported via `use`, and scoped to the importing file.

### Syntax

```emath
notation infixl 40 "⋅" => core::math::dot
notation prefix 80 "¬" => core::logic::not alias "!"
```

The production is:

```ebnf
notation_decl = "notation" , notation_fixity , integer , string , "=>" , path , [ "alias" , string ] ;
notation_fixity = "prefix" | "postfix" | "infixl" | "infixr" | "infix" ;
```

The integer is the operator precedence. The string is the glyph. The
path is the canonical operator the notation resolves to. The optional
`alias` clause provides an alternative spelling (N2).

### N1 - Notation as package citizens

Notation declarations are top-level items, peers of `use` and `emath`
declarations. They are scoped to the package that declares them. A file
that does not import a notation cannot use its glyph. Importing a
notation does not import the underlying operator - that is a separate
`use`.

### N2 - Alias policy (accept-many / canon-one)

The `alias` clause provides an additional spelling for the same
canonical operator. Multiple aliases may exist across different
packages, but they all resolve to one canonical path. The canonical
path is the `=>` target; aliases are convenience spellings, not
independent operators.

### N3 - Reserved and confusable glyphs

The following glyphs are reserved by the core language and cannot be
rebound by notation declarations:

```text
+  -  *  /  ^  ==  !=  <  <=  >  >=  and  or  not
=  :=  ->  =>  ::  .  ..  ..=  ?
```

Glyphs that are visually confusable (e.g. `−` U+2212 vs `-` U+002D,
`×` U+00D7 vs `x` U+0078) are admitted but the compiler emits a
confusable-glyph warning. The canonical ASCII operator always retains
its meaning.

### N4 - Total conflict rules

Two notation declarations conflict when they bind the same glyph with
different target paths in the same scope. The compiler refuses with
`E-NOTATION-AMBIG`. Precedence and fixity differences do not resolve
the conflict - the glyph must map to exactly one operator per scope.

A notation that shadows a reserved glyph (N3) is refused with
`E-NOTATION-RESERVED`.

### N5 - Notation⊥worlds invariant

Notation is typography, not meaning. Removing or adding a notation
import from a file does not change the semantic identity of any
declaration in that file. The semantic IR is notation-agnostic: the
same operator call admits regardless of which glyph was used to invoke
it. Notation affects only how source text is parsed, not how it is
compiled or evaluated.

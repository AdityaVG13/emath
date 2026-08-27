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

Implemented today: an import-only source can resolve selected symbols from the
embedded curated law packages, beginning with
`use physics::classical::{NewtonSecond, Hooke}`. Unknown symbols are
`E-PKG-053`; aliases and multi-package imports remain outside this narrow
embedded-package slice. The obsolete unversioned spelling
`use physics::NewtonSecond` refuses with `E-PKG-052` instead of guessing a
package. General registry and lock-file resolution has not landed.

The embedded paths are `physics::classical`, `physics::relativity`,
`cs::laws`, `probability::laws`, `analysis::laws`,
`number_theory::laws`, and `optimization_control::laws`. Importing one
selected symbol or a brace-list from one path retains its executable examples
and law metadata.

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
notation prefix 80 "¬" => core::logic::not alias "neg"
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
+  -  *  /  //  ^  ==  !=  <  <=  >  >=  and  or  not
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

### N6 - Precedence scale and binding

The core language owns a fixed lexical ladder. From loosest to
tightest, tiers 1 to 10 are:

```text
1 iff    2 implies    3 or (|)    4 and (&)    5 comparisons (== != < <= > >=)
6 unit/dimension of    7 additive (+ -)    8 multiplicative (* /)
9 unary (- + not)    10 power (^) and postfix
```

Custom operators occupy the tier at and above 11:
[`CUSTOM_OP_MIN_PRECEDENCE`] is 11, and any `notation` declaration
whose integer precedence is below the floor is refused with
`E-NOTATION-PRECEDENCE` — a lower number would parse without ever
binding, because the custom-operator infix layer only considers
declarations at or above the floor.

Binding consequences (all overridable with parentheses):

- Every custom operator binds tighter than `*` `/` and looser than
  unary prefix: `a ⊕ b * c` parses as `(a ⊕ b) * c` and `4 * x ⊕ 2`
  as `4 * (x ⊕ 2)`.
- Declared precedences order custom operators against each other:
  higher binds tighter (`x ⊕ y ⊙ z` groups `(y ⊙ z)` when `⊙`'s
  number is higher). Equal numbers chain left for `infixl`/`infix`
  and right for `infixr`.
- Prefix glyphs bind at the unary level (alongside `-`/`not`), tighter
  than custom infix; postfix glyphs bind tightest, at the postfix
  level.
- Non-letter glyphs are their own tokens and do not glue to adjacent
  identifiers: `x⊕y` is `x ⊕ y` and `√a` is `√ a`. Word aliases and
  postfix spellings (`pw`, `inv`) are letter-idents and still need
  spaces (`x pw y`, `r inv`).
- Glyph and alias spellings must each lex as a single identifier:
  punctuation such as `!` or `++` is refused with `E-NOTATION-GLYPH`,
  and a spelling that shadows a core token (N3) with
  `E-NOTATION-RESERVED`. A keyword spelling (`if`, `or`) is refused
  rather than bound as an identifier. A glyph bound to two different
  targets in one scope is refused with `E-NOTATION-AMBIG` (N4).
- A keyword cannot be used as a package path segment, declaration
  name, or field name (`package tst.if`, `emath function if:`,
  `if: Float64` are `E-SYN-101`).

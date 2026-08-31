# Standard Symbol Catalog (SSC)

The registry artifact governing which glyphs exist, what they map to, and how
collisions are resolved. Same glyphs legitimately mean many things; worlds
make interpretation data, chosen deterministically, never silently.

Machine-readable contract: [`SYMBOL_CATALOG.json`](SYMBOL_CATALOG.json),
schema `emath.symbol-catalog v1`, regenerated deterministically with:

```
cargo run -p emath-registry --example emit_symbol_catalog > language/notation/SYMBOL_CATALOG.json
```

## Entry shape

```
glyph (NFC codepoint sequence) | fixity | precedence | default world binding
| core path it maps to | aliases (ASCII/LaTeX) | confusable class | status
```

## Three rings (who may add glyphs)

| Ring | Scope | Authority cap |
|---|---|---|
| Local | any author, in their own package | `structural` (self-declared) |
| Registry | published packages | `tested` (CI suite passes) |
| Catalog | SSC/core-prelude namespace | `certified` (full ELP) |

## Lifecycle

1. **Proposed**; quarantine; usable only via explicit
   `use notation <pack>::<glyph>`; authority `none`.
2. **Checked**; producer-distinct reviewer runs the G4 audit battery;
   promotion caps at `structural-checked`.
3. **Admitted**; full ELP; part of an edition's default notation set.
4. **Frozen/retired**; hidden from new editions per the deprecation ladder
   (`emath_core::DeprecationStage`); replayable forever.

Self-certification (proposer reviewing their own promotion) is a typed
refusal: `E-SYMBOL-CATALOG-SELF-CERTIFIED`.

## Collision policy

- **Same glyph, same meaning:** alias. The SSC records both spellings;
  canonical rendering picks one.
- **Same glyph, different meaning:** permitted (the worlds thesis) but only
  as distinct scoped notation packs with disjoint namespaces. Bare use under
  both imports is a typed ambiguity refusal (`E-SYMBOL-CATALOG-AMBIGUOUS`),
  never precedence luck.
- **Different glyphs, confusable rendering:** confusable-class check at
  admission; two entries in the same class cannot both be Admitted in the
  same default namespace (`E-SYMBOL-CATALOG-CONFLUSABLE`).
- **Precedence collisions:** precedence is per-pack and explicit; importing
  two packs with overlapping operators requires an explicit combined
  precedence table, else refusal.

## Seed entries

The source declarations are retained as an executable fixture at
[`tests/fixtures/language/intro/notation-ops.emath`](../../tests/fixtures/language/intro/notation-ops.emath).
Entries begin at `Proposed` and become `Admitted` only through the catalog's
review policy.

| glyph | fixity | prec | core path | aliases | pack | status |
|---|---|---:|---|---|---|---|
| ⊕ | infixl | 40 | `core::math::pow` | `pw` | tst.notation_ops | proposed |
| √ | prefix | 80 | `core::math::sqrt` |; | tst.notation_ops | proposed |
| inv | postfix | 90 | `core::math::recip` |; | tst.notation_ops | proposed |

No-claim: full Unicode NFC verification is not implemented in std; glyphs
are authored NFC and the loader checks structural well-formedness only. A
dedicated normalization gate lands with the notation-core governance bead.

# Chapter 2: Lexical, Layout and Source Rules

## Encoding

- Source is UTF-8.
- Canonical line ending is LF.
- A byte-order mark is rejected in canonical packages.
- Identifiers use Unicode XID rules and are normalized to NFC for identity; original spelling remains available for diagnostics.
- Confusable identifier linting is enabled by default for public declarations.

## Comments and documentation

```emath
# ordinary line comment
/// documentation attached to the following item
```

Block comments may be added only with a fully nested, bounded grammar; Phase 1 uses line comments.

## Layout

Indentation is semantic after a header ending in `:`. The canonical indentation unit is four spaces. Tabs are rejected in canonical source.

`NEWLINE` is suppressed:

- inside `()`, `[]`, `{}`;
- after an incomplete assignment or infix operator;
- where the grammar explicitly consumes a continuation.

A formatter emits canonical indentation and never changes semantic identity.

## Literals

Supported literal families include:

```text
integer           123, 1_000
exact rational    3//7
decimal           1.25
fixed float       1.25f64
quantity          10 m, 3//2 s
complex           2 + 3i
string/character
boolean
```

Literal spelling, parsed exact value and target representation are distinct. A decimal literal does not become binary floating point until a numeric profile or explicit suffix requires it.

## Resource limits

The parser exposes configurable caps for file bytes, token count, literal bytes, identifier bytes, nesting, indentation depth and recovery nodes. Refusals carry stable codes and spans.

## Paths and modules

Paths use `::`. Filesystem paths do not define semantic package identities by themselves. Package/module mapping is declared in the manifest and loader.

# Maintaining the Language Folder

> Checklist for adding, changing, or refusing a language capability.
> Follow this whenever you make such a change.

## The four-artifact rule

Every language change updates four artifacts in the same commit:

1. **Reference chapter** (`language/reference/`) - the normative spec.
   Update the relevant chapter's "Implemented today" section and any
   prose that describes the feature. When adding a new capability, add
   it to the appropriate chapter body.

2. **Grammar** (`language/grammar/surface.ebnf`) - the machine-checkable
   surface model. Add or modify the grammar rule, with a comment
   explaining the rule's intent.

3. **Example** (`language/examples/`) - reuse an existing program when
   the feature is a new form of something already shown (`scratch`,
   `autodiff`, `heat-rod-sim`, a domain example). Add a new `.emath` file only when the
   user-visible program is genuinely a new kind of work, not a new
   keyword. Do not add one intro file per bead or diagnostic.

4. **Example index** (`language/examples/README.md`) - if you added a
   file, add one row. If you reused a file, leave the index alone.

## Additional updates

5. **Capability matrix** (`language/CAPABILITY.md`) - add or update the
   row for the new feature. This is the single-source-of-truth for what
   works today.

6. **Overview** (`language/reference/overview.md`) - update the
   "Implemented today" section if the feature adds a new type,
   expression form, or section.

7. **Tests** (`tests/`) - add focused tests that prove the feature
   works. Syntax tests in `tests/emath-syntax/tests/`, sema tests in
   `tests/emath-sema/tests/`, exec-ir tests in `tests/emath-exec-ir/tests/`.

## Order of operations

```
1. Implement the feature in the compiler (crates/)
2. Write tests that prove it works
3. Update the reference chapter
4. Update the grammar (surface.ebnf)
5. Create the example file
6. Update examples/README.md
7. Update CAPABILITY.md
8. Update overview.md if needed
9. Run full test suite
10. Commit all artifacts together
```

## Chapter map

| Feature type | Chapter |
|-------------|---------|
| New type or type modifier | ch5 (types-units-shapes-and-domains.md) |
| New expression or operator | ch7 (expressions-equations-state-and-events.md) |
| New section or declaration form | ch4 (declarations-sections-and-attributes.md) |
| New builtin function | ch7 + CAPABILITY.md builtins table |
| New lexical rule | ch2 (lexical-layout-and-source.md) |
| New package/import rule | ch3 (packages-modules-and-imports.md) |
| New notation | ch3 (notation governance section) + ch2 if lexical |
| New goal or strategy | ch9 (goals-requests-strategies-and-resolution.md) |

## Status labels

Use these consistently in examples and CAPABILITY.md:

| Label | Meaning |
|-------|---------|
| **Runs** | Parses, admits, and produces a correct result |
| **Parses** | Parses without errors but does not evaluate |
| **Admits** | Passes semantic admission but runtime not implemented |
| **Refused** | Parses but sema refuses with a named error |

## When to update CAPABILITY.md

Always. If a feature moves from "Parses" to "Runs", update the matrix.
If a type moves from "not admitted" to "admitted", update the matrix.
CAPABILITY.md is the first file a user should read to understand what
emath can do today.

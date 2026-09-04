# Generated Capability Matrix

Prototype Language Image: `sha256:5d6bda518ade3aadf96c89735f9e7029dab2d91e2f86e649cd1e40f4d89bdb43`

This is a target-spec view, not a claim that every feature is implemented.

| Class | Count |
|---|---:|
| `artifact` | 12 |
| `binder` | 7 |
| `capability` | 19 |
| `diagnostic` | 8 |
| `family` | 5 |
| `field-pack` | 5 |
| `instance` | 4 |
| `kind` | 19 |
| `lens` | 3 |
| `method` | 6 |
| `migration` | 4 |
| `section` | 15 |
| `symbol` | 10 |
| `syntax` | 8 |
| `syntax-pack` | 1 |
| `theory` | 6 |
| `type` | 15 |
| `world` | 9 |

## Features

| FeatureID | Class | Maturity | Owner | Summary |
|---|---|---|---|---|
| `std.artifact.cargo@1` | artifact | proposed | `emath-artifact` | Generated Rust/Cargo component and semantic manifest. |
| `std.artifact.certificate@1` | artifact | proposed | `emath-artifact` | Witness plus bounded independent checker. |
| `std.artifact.continuation@1` | artifact | proposed | `emath-artifact` | Resumable bounded work. |
| `std.artifact.diagnostic@1` | artifact | proposed | `emath-artifact` | Typed unresolved or fault result with routes. |
| `std.artifact.experiment@1` | artifact | proposed | `emath-artifact` | Frozen experiment, raw/derived evidence, and decision. |
| `std.artifact.proof_obligation@1` | artifact | proposed | `emath-artifact` | Exact statement for a proof provider. |
| `std.artifact.research_state@1` | artifact | proposed | `emath-artifact` | V18 criterion/obstruction/candidate research state. |
| `std.artifact.source@1` | artifact | proposed | `emath-artifact` | Exact source bytes, glyphs, edition, and provenance. |
| `std.artifact.symbolic@1` | artifact | proposed | `emath-artifact` | Canonical symbolic form. |
| `std.artifact.value@1` | artifact | proposed | `emath-artifact` | Concrete or structured value with exactness label. |
| `std.artifact.wasm@1` | artifact | proposed | `emath-artifact` | Generated WebAssembly component and semantic manifest. |
| `std.artifact.world_portfolio@1` | artifact | proposed | `emath-artifact` | Several labeled world results without hidden collapse. |
| `std.binder.exists@1` | binder | proposed | `emath-syntax` | Existential proposition or witness goal. |
| `std.binder.forall@1` | binder | proposed | `emath-syntax` | Universal proposition over a declared domain. |
| `std.binder.integral@1` | binder | proposed | `emath-syntax` | Integral over a measure/domain with explicit method. |
| `std.binder.limit@1` | binder | proposed | `emath-syntax` | Limit goal under a declared topology/direction. |
| `std.binder.product@1` | binder | proposed | `emath-syntax` | Multiplicative fold over a finite or method-supported domain. |
| `std.binder.series@1` | binder | proposed | `emath-syntax` | Finite or convergent series with explicit convergence semantics. |
| `std.binder.sum@1` | binder | proposed | `emath-syntax` | Additive fold over a finite or method-supported domain. |
| `std.capability.aggregate.product@1` | capability | proposed | `emath-term` | Multiplicative aggregation over a domain. |
| `std.capability.aggregate.sum@1` | capability | proposed | `emath-term` | Additive aggregation over a domain. |
| `std.capability.analysis.integral@1` | capability | proposed | `emath-term` | Integral with explicit method/evidence. |
| `std.capability.analysis.limit@1` | capability | proposed | `emath-term` | Limit under declared topology and direction. |
| `std.capability.analysis.series@1` | capability | proposed | `emath-term` | Series evaluation with convergence contract. |
| `std.capability.coding.rs_encode@1` | capability | proposed | `emath-term` | Exact polynomial evaluation codeword. |
| `std.capability.evidence.check@1` | capability | proposed | `emath-term` | Check evidence within its authority. |
| `std.capability.field.mod_inverse@1` | capability | proposed | `emath-term` | Exact modular inverse when gcd=1. |
| `std.capability.field.mod_power@1` | capability | proposed | `emath-term` | Exact modular exponentiation. |
| `std.capability.goal.solve@1` | capability | proposed | `emath-term` | Construct a typed solve goal without hidden method selection. |
| `std.capability.logic.exists@1` | capability | proposed | `emath-term` | Existential proposition or witness goal. |
| `std.capability.logic.forall@1` | capability | proposed | `emath-term` | Universal proposition/evaluation over supported domains. |
| `std.capability.logic.implies@1` | capability | proposed | `emath-term` | Logical implication. |
| `std.capability.math.add@1` | capability | proposed | `emath-term` | Typed addition selected by an instance. |
| `std.capability.math.divide@1` | capability | proposed | `emath-term` | Typed division with zero/noninvertible handling. |
| `std.capability.math.multiply@1` | capability | proposed | `emath-term` | Typed multiplication. |
| `std.capability.math.power@1` | capability | proposed | `emath-term` | Power under a resolved theory instance. |
| `std.capability.math.subtract@1` | capability | proposed | `emath-term` | Typed subtraction. |
| `std.capability.tensor.softmax@1` | capability | proposed | `emath-term` | Stable normalized exponential. |
| `std.diagnostic.artifact.overclaim@1` | diagnostic | proposed | `error registry` | Artifact label exceeds evidence. |
| `std.diagnostic.authority.dual@1` | diagnostic | proposed | `error registry` | Two sources claim authority for one FeatureID. |
| `std.diagnostic.exactness.loss@1` | diagnostic | proposed | `error registry` | Exact request would silently round/demote. |
| `std.diagnostic.projection.missing@1` | diagnostic | proposed | `error registry` | Required feature projection is absent/stale. |
| `std.diagnostic.section.invalid@1` | diagnostic | proposed | `error registry` | Section violates owning kind schema. |
| `std.diagnostic.spec_hole.blocking@1` | diagnostic | proposed | `error registry` | Stable generation crosses unresolved semantics. |
| `std.diagnostic.symbol.unresolved@1` | diagnostic | proposed | `error registry` | Strict symbol has no FeatureID. |
| `std.diagnostic.world.unsupported@1` | diagnostic | proposed | `error registry` | World lacks required capability. |
| `std.family.constraint_puzzle@1` | family | proposed | `emath-schema` | Generates puzzle kinds from variables, domains, constraints, verifier, and solution policy. |
| `std.family.criterion_edge@1` | family | proposed | `emath-schema` | Generates V18 criterion-edge features with direction and authority. |
| `std.family.elementwise_unary@1` | family | proposed | `emath-schema` | Generates scalar/tensor unary capabilities from function, domain, and derivative metadata. |
| `std.family.execution_transform@1` | family | proposed | `emath-schema` | Generates V17 schedule transformations with preconditions and evidence. |
| `std.family.finite_cipher@1` | family | proposed | `emath-schema` | Generates finite-alphabet cipher capabilities, round-trip laws, and analysis fixtures. |
| `std.field_pack.conjecture_dynamics@1` | field-pack | proposed | `emath-store` | V18 frontiers, criteria, research worlds, and research-state artifacts. |
| `std.field_pack.core@1` | field-pack | proposed | `emath-store` | Core syntax, types, capabilities, worlds, and artifacts. |
| `std.field_pack.cryptology@1` | field-pack | proposed | `emath-store` | V15 ciphers, puzzles, codes, protocols, worlds, methods, and evidence. |
| `std.field_pack.exact_algebra@1` | field-pack | proposed | `emath-store` | Exact rings/fields, algebraic structures, vectors, matrices, and methods. |
| `std.field_pack.execution_dynamics@1` | field-pack | proposed | `emath-store` | V17 workloads, schedules, metrics, campaigns, and measured worlds. |
| `std.instance.gf.field@1` | instance | proposed | `emath-world-ir` | Prime field for literal prime p. |
| `std.instance.int.add_monoid@1` | instance | proposed | `emath-world-ir` | Integer addition monoid. |
| `std.instance.int.ring@1` | instance | proposed | `emath-world-ir` | Exact integer ring. |
| `std.instance.matrix.mul_monoid@1` | instance | proposed | `emath-world-ir` | Square-matrix multiplication when R is a semiring. |
| `std.kind.capability@1` | kind | proposed | `emath-schema` | Defines a typed capability cell. |
| `std.kind.cipher@1` | kind | proposed | `emath-schema` | Defines message/key types, encrypt/decrypt, correctness, security, and providers. |
| `std.kind.code@1` | kind | proposed | `emath-schema` | Defines encoder, channel/noise, decoder, and recovery guarantees. |
| `std.kind.criterion@1` | kind | proposed | `emath-frontier` | Defines a directional relation between statements with scoped evidence. |
| `std.kind.custom@1` | kind | proposed | `emath-genesis` | Open-semantics declaration preserving unknown symbols and world portfolios. |
| `std.kind.family@1` | kind | proposed | `emath-schema` | Generates regular feature capsules from a bounded template. |
| `std.kind.field_pack@1` | kind | proposed | `emath-store` | Defines an installable semantic feature bundle. |
| `std.kind.frontier@1` | kind | proposed | `emath-frontier` | Defines an unresolved-problem research programme and research-state output. |
| `std.kind.function@1` | kind | proposed | `emath-schema` | Typed mathematical mapping with definitions, goals, tests, and artifacts. |
| `std.kind.instance@1` | kind | proposed | `emath-world-ir` | Implements a theory over a carrier. |
| `std.kind.kind@1` | kind | proposed | `emath-schema` | Defines a bounded schema-driven declaration kind. |
| `std.kind.method@1` | kind | proposed | `emath-plan` | Defines a strategy for satisfying a typed goal. |
| `std.kind.model@1` | kind | proposed | `emath-schema` | State, parameters, equations, transitions, events, and observations. |
| `std.kind.performance_campaign@1` | kind | proposed | `emath-lab-core` | Defines baseline, candidates, workloads, targets, gates, metrics, and promotion. |
| `std.kind.policy@1` | kind | proposed | `emath-schema` | Decision/scoring component with state and protected behavior. |
| `std.kind.protocol@1` | kind | proposed | `emath-schema` | Defines roles, messages, channels, knowledge, claims, and traces. |
| `std.kind.puzzle@1` | kind | proposed | `emath-schema` | Defines variables, clues, constraints, solution, verification, and generation. |
| `std.kind.theory@1` | kind | proposed | `emath-world-ir` | Defines abstract operations and laws. |
| `std.kind.world@1` | kind | proposed | `emath-world-ir` | Defines an explicit interpretation, capabilities, strategy, effects, and evidence. |
| `std.lens.agent@1` | lens | proposed | `emath-lsp` | Compact FeatureIDs, dependencies, gates, owners, and selective expansion. |
| `std.lens.formal@1` | lens | proposed | `emath-lsp` | Explicit types, theories, assumptions, worlds, and obligations. |
| `std.lens.user@1` | lens | proposed | `emath-lsp` | Readable canonical source and concise results. |
| `std.method.measure.paired@1` | method | proposed | `emath-plan` | Paired baseline/candidate measurement. |
| `std.method.proof.external@1` | method | proposed | `emath-plan` | Send exact obligations to a proof provider. |
| `std.method.research.counterexample@1` | method | proposed | `emath-plan` | Find a counterexample within a scoped model/domain. |
| `std.method.search.exhaustive@1` | method | proposed | `emath-plan` | Exact witness or counterexample search on finite domains. |
| `std.method.solve.newton@1` | method | proposed | `emath-plan` | Root solving for differentiable scalar functions. |
| `std.method.world.portfolio@1` | method | proposed | `emath-plan` | Evaluate one admitted term across several explicit worlds. |
| `std.migration.capability_manual_to_generated@1` | migration | proposed | `emath-migrate` | Migrate manual CAPABILITY view to generated orthogonal view. |
| `std.migration.cypher_to_cipher@1` | migration | proposed | `emath-migrate` | Migrate cypher spelling to cipher spelling. |
| `std.migration.domain_grammar_to_pack@1` | migration | proposed | `emath-migrate` | Migrate domain grammar production to syntax-pack expansion. |
| `std.migration.reference_to_capsule@1` | migration | proposed | `emath-migrate` | Migrate legacy reference authority to Feature Capsule authority. |
| `std.section.budget@1` | section | proposed | `emath-schema` | Time, memory, search, precision, and provider budgets. |
| `std.section.compile@1` | section | proposed | `emath-schema` | Artifact target, numeric representation, and host policy. |
| `std.section.constraints@1` | section | proposed | `emath-schema` | Mathematical constraints and validity predicates. |
| `std.section.constructors@1` | section | proposed | `emath-schema` | Valid-state constructors and obligations. |
| `std.section.definitions@1` | section | proposed | `emath-schema` | Definitions and equations stating mathematical meaning. |
| `std.section.equations@1` | section | proposed | `emath-schema` | Implicit equations and residual relations. |
| `std.section.evidence@1` | section | proposed | `emath-schema` | Evidence requirements, sources, and authority ceilings. |
| `std.section.goals@1` | section | proposed | `emath-schema` | Requested work, separate from definitions. |
| `std.section.inputs@1` | section | proposed | `emath-schema` | Typed inputs and parameters. |
| `std.section.methods@1` | section | proposed | `emath-schema` | Permitted/preferred solution strategies. |
| `std.section.outputs@1` | section | proposed | `emath-schema` | Typed named outputs. |
| `std.section.provenance@1` | section | proposed | `emath-schema` | Source and derivation lineage. |
| `std.section.state@1` | section | proposed | `emath-schema` | State variables and initial values. |
| `std.section.tests@1` | section | proposed | `emath-schema` | Embedded examples and expectations. |
| `std.section.worlds@1` | section | proposed | `emath-schema` | Explicit world selection or portfolio policy. |
| `std.symbol.definition@1` | symbol | proposed | `emath-syntax` | Defines a binding in definition contexts; proposition equality is separate. |
| `std.symbol.equality.approx@1` | symbol | proposed | `emath-syntax` | Forms an explicitly bounded approximate relation. |
| `std.symbol.equality.exact@1` | symbol | proposed | `emath-syntax` | Forms an exact equality proposition. |
| `std.symbol.logic.implies@1` | symbol | proposed | `emath-syntax` | Logical implication. |
| `std.symbol.math.add@1` | symbol | proposed | `emath-syntax` | Addition selected through type/theory resolution. |
| `std.symbol.math.divide@1` | symbol | proposed | `emath-syntax` | Division selected through type/theory resolution. |
| `std.symbol.math.multiply@1` | symbol | proposed | `emath-syntax` | Multiplication selected through type/theory resolution. |
| `std.symbol.math.power@1` | symbol | proposed | `emath-syntax` | Power selected by base theory and exponent type. |
| `std.symbol.math.subtract@1` | symbol | proposed | `emath-syntax` | Subtraction or additive inverse selected through type/theory resolution. |
| `std.symbol.membership@1` | symbol | proposed | `emath-syntax` | Membership relation and binder-domain connective. |
| `std.syntax.binder.generic@1` | syntax | accepted | `emath-syntax` | Provides the universal binder skeleton. |
| `std.syntax.declaration.generic@1` | syntax | accepted | `emath-syntax` | Parses `emath <kind> Name: suite` without domain knowledge. |
| `std.syntax.expression.core@1` | syntax | accepted | `emath-syntax` | Provides literals, paths, calls, collections, records, indexing, and registered operators. |
| `std.syntax.glyph.unknown@1` | syntax | accepted | `emath-syntax` | Preserves unregistered glyphs without inventing meaning. |
| `std.syntax.let@1` | syntax | accepted | `emath-syntax` | Introduces a hygienic immutable local binding. |
| `std.syntax.section.generic@1` | syntax | accepted | `emath-syntax` | Parses a named section whose meaning is supplied by its kind schema. |
| `std.syntax.source@1` | syntax | accepted | `emath-syntax` | Losslessly preserves UTF-8 source and layout. |
| `std.syntax.use@1` | syntax | accepted | `emath-syntax` | Imports names, packs, worlds, methods, and syntax under a lock. |
| `std.syntax_pack.scratch@1` | syntax-pack | proposed | `emath-syntax` | Expands zero-ceremony expressions and intent lines into visible canonical declarations. |
| `std.theory.field@1` | theory | proposed | `emath-world-ir` | Commutative field structure. |
| `std.theory.magma@1` | theory | proposed | `emath-world-ir` | A carrier with one binary operation. |
| `std.theory.monoid@1` | theory | proposed | `emath-world-ir` | Associative operation with identity. |
| `std.theory.ring@1` | theory | proposed | `emath-world-ir` | Ring structure. |
| `std.theory.semiring@1` | theory | proposed | `emath-world-ir` | Additive and multiplicative structure for linear algebra. |
| `std.theory.world@1` | theory | proposed | `emath-world-ir` | Contract shared by curated worlds. |
| `std.type.bigint@1` | type | proposed | `emath-core` | Arbitrary-precision signed integers. |
| `std.type.bool@1` | type | proposed | `emath-core` | Boolean truth values. |
| `std.type.complex@1` | type | proposed | `emath-core` | Complex values over a declared component representation. |
| `std.type.float64@1` | type | proposed | `emath-core` | IEEE binary64 numeric representation. |
| `std.type.gf@1` | type | proposed | `emath-core` | Prime finite field values for literal prime p. |
| `std.type.int@1` | type | proposed | `emath-core` | Signed exact integers with explicit overflow lane. |
| `std.type.interval@1` | type | proposed | `emath-core` | Certified enclosure over an ordered representation. |
| `std.type.matrix@1` | type | proposed | `emath-core` | Fixed-shape matrix over an admitted coefficient type. |
| `std.type.meaning_hole@1` | type | proposed | `emath-core` | Unresolved meaning with constraints and state. |
| `std.type.nat@1` | type | proposed | `emath-core` | Nonnegative exact integers. |
| `std.type.option@1` | type | proposed | `emath-core` | Explicit optional value. |
| `std.type.rational@1` | type | proposed | `emath-core` | Canonical reduced rational values. |
| `std.type.result@1` | type | proposed | `emath-core` | Explicit success/failure value. |
| `std.type.tensor@1` | type | proposed | `emath-core` | Typed tensor with explicit shape/layout. |
| `std.type.vector@1` | type | proposed | `emath-core` | Fixed-length vector over an admitted element type. |
| `std.world.exact.int@1` | world | proposed | `emath-world-ir` | Execute exact integer semantics with explicit overflow lane. |
| `std.world.exact.prime_field@1` | world | proposed | `emath-world-ir` | Execute exact prime-field arithmetic. |
| `std.world.exact.rational@1` | world | proposed | `emath-world-ir` | Execute exact reduced rational arithmetic. |
| `std.world.finite.canonical@1` | world | proposed | `emath-world-ir` | Assign deterministic total finite operations without intended-meaning claim. |
| `std.world.numeric.interval@1` | world | proposed | `emath-world-ir` | Propagate certified enclosures. |
| `std.world.numeric.strict_f64@1` | world | proposed | `emath-world-ir` | Execute deterministic binary64 policy. |
| `std.world.performance.measured@1` | world | proposed | `emath-world-ir` | Inject trusted performance samples. |
| `std.world.research.criterion@1` | world | proposed | `emath-world-ir` | Produce criterion and proof-obligation artifacts. |
| `std.world.symbolic.free@1` | world | proposed | `emath-world-ir` | Construct operations structurally without intended meaning. |

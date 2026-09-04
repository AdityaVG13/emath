# `std.geometry.world.inner-product`; inner-product world over R^3

Status: language-layer capability cell (wave-16 worlds lane, bead
emath-wave16-catalog-epic-fassw.26, catalog item **inner-product
world** — "vector space with inner product and induced geometry").
Realized as user-declared `.emath` over (R^3, dot): the world's laws
are executable witnesses, its induced geometry (norm, angle,
parallelogram, polarization) computes today.  ZERO CORE DELTA.

## What computes (exact-first)

- Inner product `dot(u, v)` over Vector[3]; axioms as pinned signed
  slacks: symmetry `dot(a,b) - dot(b,a) = 0`, additivity
  `dot(a+c, b) - dot(a,b) - dot(c,b) = 0`, scale homogeneity
  `dot(a, t·b) - t·dot(a,b) = 0` — all exact for integer vectors and
  dyadic `t`.
- Cauchy-Schwarz with equality case: `dot(a,a)·dot(b,b) - dot(a,b)²
  = 75 > 0` for independent probes, `= 0` exactly for `b = 3a` (the
  equality case is part of the witness, not omitted).
- Induced geometry: angle `cos = dot/(‖a‖·‖b‖)` (one correctly
  rounded literal, `3/sqrt(84) = 0.3273268353539886`, verified
  independently); parallelogram law `‖a+b‖² + ‖a-b‖² - 2‖a‖² -
  2‖b‖² = 0` and polarization identity `‖a+b‖² - ‖a-b‖² -
  4·dot(a,b) = 0`, both exact at dyadic scales; norm homogeneity
  `‖t·v‖ - t·‖v‖ = 0` exact for dyadic `t` on Pythagorean carriers
  (norm(3,4,0) = 5, norm(6,8,0) = 10 bit-exact).

## LAW ENCODING FENCE (encoding, not execution)

Evaluate targets are strict-f64 values; the language has no boolean
evaluate targets and no comparison operators in this subset, so every
axiom/inequality is encoded as a signed slack (0 = identity exactly,
negative = strict inequality).  This is a fixed encoding convention
of the cell, documented here, not a silent limitation.

## NAMED FENCES (world machinery, execution refused today)

- Native `world` declaration (genesis carriers/symbols/laws sections)
  is design-only per `language/grammar/genesis.ebnf`; the parser
  refuses unadmitted sections.
- Universal quantification over a carrier ("for all u, v") is a world
  machinery obligation, not expressible today; witnesses are pinned
  probes.
- Gram/Schmidt and orthogonality projection beyond the pinned
  witnesses wait on nothing mathematical — they are extensions of
  this same cell shape and land when the lane re-opens the file.

## Test shape

Each test-bearing declaration observes exactly one evaluate target;
slacks are pinned exactly (0.0 for identities, signed values for
inequalities); the single irrational angle pin is data, not
computation.  Mutation-checked: 1-ULP angle-pin mutant, 2.1
parallelogram-coefficient mutant, and `+1.0` homogeneity-slack
mutant are all killed by their owning witnesses.

## Contracts

- Pure, deterministic, strict-f64; zero core delta.
- Runnable artifact:
  `language/examples/geometry/worlds/inner-product-world.emath`
  (check + generated-crate tests pass).
- No-claim boundary: the cell pins the axioms on pinned probes; it
  does not provide a proof system for the axioms over arbitrary
  vectors, and it does not claim the angle beyond the correctly
  rounded literal it pins.

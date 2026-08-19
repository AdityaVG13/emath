# CONTRACT — emath-agent-protocol

## Purpose and layer
- Layer: Tier 7, governance and operations (per implementation/CRATE_MAP.md).
- Agent-native meaning proposals: submission envelope, admission, challenge loop, feedback, capability gates.
- A proposal traverses: schema admission, capability admission, deterministic checker suite, counterexample generation, evidence and cost gates, portfolio ranking, then revision request or world candidate.
- Depends on: emath-term, emath-world-ir, emath-tuning, emath-portfolio.

## Public types and semantics
- `AgentProposal` - one proposal envelope (problem id, base world or hole ids, changes, claimed obligations, derivation, required providers, estimated cost, requested authority).
- `ProposalKind` - what kind of proposal it is.
- `ChallengeLoop` - runs the deterministic challenge over a proposal against a portfolio; returns `ChallengeOutcome`.
- `ChallengeOutcome` - `Refused(AdmissionRefusal)` or a world candidate / revision request.
- `CheckerSuite` / `NamedCheck` - ordered deterministic checks over a proposal.
- `AdmissionRefusal` - stable admission refusal (code, detail, proposal identity).
- `AgentFeedback` - structured feedback to the agent (solved holes, failed constraints, counterexample, unmet evidence, cost regression, portfolio dominance).
- `EXECUTION_AUTHORITIES` / `PROPOSAL_AUTHORITIES` - the two authority namespaces agents may request.
- (not exhaustive)

## Invariants
- A proposal carries no direct execution authority; the loop never grants execution authority and never grants `Certified` or `Proved`.
- `Certified`/`Proved` require external compiler, capability, evidence, and benchmark gates, not the challenge loop.
- Challenge runs checks in deterministic order.
- A proposal either yields a revision request or a world candidate, never a granted authority.

## Error model
- Admission failures surface as typed `AdmissionRefusal` (machine-readable `code`, human-readable `detail`, `proposal_identity`); exposed as `ChallengeOutcome::Refused`.
- Individual checks return `Result<(), String>`; the first failure text is surfaced as the smallest counterexample.
- No panics; no untyped string fallbacks for admission.

## Determinism class
- Deterministic: schema admission, capability admission, checker suite, counterexample generation, evidence and cost gates, and portfolio ranking are ordered and canonical; proposal and feedback carry canonical forms.

## Cancellation behavior
- Not applicable: std-only synchronous crate, no cancellation surface.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]` and the workspace lint forbid `unsafe_code`.

## Feature flags
- None: Cargo.toml has no `[features]`.

## Conformance tests
- `src/challenge.rs` `#[cfg(test)]`:
  - admission refuses an execution-authority claim with `capability:authority-not-admitted`
  - admission refuses an incomplete schema (missing base worlds) with `schema:incomplete`
  - a valid proposal runs to a `WorldCandidate` whose proposal identity and candidate identity are deterministic across two constructions of the same envelope

## No-claim boundaries
- A slice of the planned governance surface, not the full production admission service.
- Meaning proposals are proposals, not certified answers; their authority claims are requests, not grants.
- World candidates from the loop are ranked by portfolio, not certified by admission.

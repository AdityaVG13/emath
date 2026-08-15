#![forbid(unsafe_code)]

//! V7 g9 — Agent-native meaning proposals (spec 18).
//!
//! Agents may propose parse hypotheses, signatures, carriers, operator
//! meanings, laws, constructors, world deltas, selection policies, and
//! implementation plans. Every proposal travels in a submission envelope
//! (problem ID, base world or hole IDs, proposed changes, claimed
//! obligations, supporting derivation, required providers, estimated
//! cost, requested authority) and enters the challenge loop:
//!
//! ```text
//! agent proposal
//!     → schema admission
//!     → deterministic checker suite
//!     → counterexample generation
//!     → revision request or world candidate
//!     → portfolio ranking
//! ```
//!
//! Agent proposals carry no direct execution authority: source code is not
//! inserted into a host binary without the separate compiler, capability,
//! evidence, and benchmark gates, and a proposal result can never be
//! granted `Certified` or `Proved` authority by the loop itself.

pub mod challenge;
pub mod proposal;

pub use challenge::{
    AdmissionRefusal, AgentFeedback, ChallengeLoop, ChallengeOutcome, CheckerSuite, NamedCheck,
    RevisionRequest, WorldCandidateRef,
};
pub use proposal::{AgentProposal, ProposalKind, EXECUTION_AUTHORITIES, PROPOSAL_AUTHORITIES};

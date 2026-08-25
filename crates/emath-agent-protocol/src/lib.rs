#![forbid(unsafe_code)]

//! Agent-native meaning proposals.
//!
//! Proposals travel in a submission envelope through the challenge loop:
//! schema admission, deterministic checker suite, counterexample
//! generation, revision request or world candidate, portfolio ranking.
//! The loop never grants execution authority or `Certified`/`Proved`.

pub mod challenge;
pub mod proposal;

pub use challenge::{
    AdmissionRefusal, AgentFeedback, ChallengeLoop, ChallengeOutcome, CheckerSuite, NamedCheck,
    RevisionRequest, WorldCandidateRef,
};
pub use proposal::{AgentProposal, EXECUTION_AUTHORITIES, PROPOSAL_AUTHORITIES, ProposalKind};

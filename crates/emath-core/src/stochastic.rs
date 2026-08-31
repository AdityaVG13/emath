//! Stochastic semantics contract (thin slice, bead
//! emath-gap-stochastic-vnqo): seed identity, named algorithm, stream
//! splits, and receipt binding.
//!
//! Contract law:
//! - **Seed is identity.** The seed is an explicit part of run/campaign
//!   identity. [`Seed`] has no `Default` and there is no ambient
//!   constructor anywhere in core: a seed exists only when a run
//!   declares one. Entropy access is a declared capability (C10), never
//!   an ambient side effect — [`E_STOCH_ENTROPY`] is the typed refusal
//!   code for undeclared entropy access.
//! - **Named algorithm.** The generator of record is a counter-based
//!   Philox-class function ([`ALGORITHM_PHILOX4X32_10`]); algorithm
//!   identity is a closed gate — an unknown name refuses
//!   ([`E_STOCH_ALGORITHM`]) rather than silently swapping generators
//!   behind the same receipt.
//! - **Stream splits are declared paths.** A [`StreamPath`] is the
//!   declared split topology; changing the topology changes the stream,
//!   while call order (parallel or sequential execution) never does.
//! - **Receipts bind (seed, algorithm, stream path).** A
//!   [`StochasticReceipt`] carries the exact triple with a content id,
//!   so any stochastic answer names how to replay it.
//!
//! Distribution semantics (sampling procedures, parameterizations) are
//! world meanings on the distributions lane; they consume
//! [`stream_value`] — the single deterministic primitive two providers
//! must share for "same world + same seed ⇒ same stream".

use crate::hash::fnv1a64_bytes;

/// `E-STOCH-1` — an algorithm name that is not the declared generator.
pub const E_STOCH_ALGORITHM: &str = "E-STOCH-1";
/// `E-STOCH-2` — a malformed declared stream path (empty label).
pub const E_STOCH_STREAM: &str = "E-STOCH-2";
/// `E-STOCH-3` — undeclared entropy access (ambient randomness is a
/// capability refusal, never silent pseudo-randomness).
pub const E_STOCH_ENTROPY: &str = "E-STOCH-3";

/// The named counter-based generator of record: Philox4x32 with 10
/// rounds (Random123 construction). Identity is RECORDED in receipts;
/// providers may exist only behind this declared name.
pub const ALGORITHM_PHILOX4X32_10: &str = "philox4x32-10";

// Philox4x32 constants (Random123): odd multipliers and Weyl increments.
const PHILOX_M0: u32 = 0xD2511F53;
const PHILOX_M1: u32 = 0xCD9E8D57;
const PHILOX_W0: u32 = 0x9E3779B9;
const PHILOX_W1: u32 = 0xBB67AE85;

/// Explicit seed identity. Deliberately NOT `Default`, `From<u64>` for
/// ambient use is the only constructor, and no core function generates
/// one: the seed is always run-declared data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seed(u64);

impl Seed {
    pub fn new(value: u64) -> Self {
        Seed(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for Seed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A declared stream path: the split topology that names one deterministic
/// sub-stream of a seeded run. The empty path is the legal ROOT stream;
/// each split label must be non-empty. Labels are ORDERED — `a.b` and
/// `b.a` are different streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamPath {
    labels: Vec<String>,
}

impl StreamPath {
    /// The root stream (no declared splits yet).
    pub fn root() -> Self {
        StreamPath { labels: Vec::new() }
    }

    pub fn new(labels: Vec<String>) -> Result<Self, String> {
        for (i, label) in labels.iter().enumerate() {
            if label.is_empty() {
                return Err(format!("{E_STOCH_STREAM}: label {i} must be non-empty"));
            }
        }
        Ok(StreamPath { labels })
    }

    /// One declared split deeper.
    pub fn child(&self, label: &str) -> StreamPath {
        let mut labels = self.labels.clone();
        labels.push(label.to_string());
        StreamPath { labels }
    }

    /// Canonical dotted form (empty string for the root stream).
    pub fn canonical(&self) -> String {
        self.labels.join(".")
    }
}

/// The single deterministic stream primitive the contract offers:
/// `stream_value(seed, algorithm, path, counter) -> u64`. Pure in all
/// arguments — parallel and sequential execution of any query order
/// yields identical values.
///
/// Declared mapping (part of the contract, documented in the cell
/// contract): Philox4x32-10 rounds with the seed split into the two key
/// words, the FNV-1a64 hash of the canonical stream path into counter
/// words 0-1, and the query counter into counter words 2-3. The stream is
/// therefore a pure function of the identity triple plus the counter.
pub fn stream_value(
    seed: &Seed,
    algorithm: &str,
    path: &StreamPath,
    counter: u64,
) -> Result<u64, String> {
    if algorithm != ALGORITHM_PHILOX4X32_10 {
        return Err(format!(
            "{E_STOCH_ALGORITHM}: algorithm `{algorithm}` is not the declared generator \
             ({ALGORITHM_PHILOX4X32_10}); receipts cannot bind an unnamed generator"
        ));
    }
    let path_hash = fnv1a64_bytes(path.canonical().as_bytes());
    let mut c = [
        path_hash as u32,
        (path_hash >> 32) as u32,
        counter as u32,
        (counter >> 32) as u32,
    ];
    let mut k = [seed.value() as u32, (seed.value() >> 32) as u32];
    for _ in 0..10 {
        let p0 = u64::from(PHILOX_M0) * u64::from(c[0]);
        let p1 = u64::from(PHILOX_M1) * u64::from(c[2]);
        let hi0 = (p0 >> 32) as u32;
        let lo0 = p0 as u32;
        let hi1 = (p1 >> 32) as u32;
        let lo1 = p1 as u32;
        c = [hi1 ^ c[1] ^ k[0], lo1, hi0 ^ c[3] ^ k[1], lo0];
        k[0] = k[0].wrapping_add(PHILOX_W0);
        k[1] = k[1].wrapping_add(PHILOX_W1);
    }
    Ok((u64::from(c[0]) << 32) | u64::from(c[2]))
}

/// The canonical seed word for STATEFUL LOCAL generators (SplitMix64-class
/// stepping generators that keep a `u64` state): counter 0 of the declared
/// stream. This is the contract's one-seed-story seam — a local generator
/// never owns its own seed namespace; its initial state derives from the
/// same `(Seed, StreamPath)` identity every other stream consumer uses.
/// The pre-contract provisional mapping (raw seed bits as the state)
/// re-maps to this function without touching the generators themselves.
pub fn local_stream_seed(seed: &Seed, path: &StreamPath) -> Result<u64, String> {
    stream_value(seed, ALGORITHM_PHILOX4X32_10, path, 0)
}

/// The replay record: (seed, algorithm, stream path). A stochastic answer
/// that cites this receipt is replayable to the byte by re-deriving the
/// same streams under the same identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StochasticReceipt {
    seed: u64,
    algorithm: String,
    stream: String,
}

impl StochasticReceipt {
    pub fn new(seed: &Seed, algorithm: &str, stream: &StreamPath) -> Self {
        StochasticReceipt {
            seed: seed.value(),
            algorithm: algorithm.to_string(),
            stream: stream.canonical(),
        }
    }

    /// Canonical one-line binding of the triple.
    pub fn canonical(&self) -> String {
        format!(
            "stochastic-receipt seed={} algorithm={} stream={}",
            self.seed, self.algorithm, self.stream
        )
    }

    /// Content id over the canonical binding.
    pub fn content_id(&self) -> String {
        format!(
            "fnv1a64:{:016x}",
            fnv1a64_bytes(self.canonical().as_bytes())
        )
    }
}

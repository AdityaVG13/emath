//! emath-gap-stochastic-vnqo (thin contract slice): seed identity, named
//! RNG algorithm, stream-split semantics, and receipt binding.
//!
//! The contract: the seed is EXPLICIT run identity (no ambient entropy —
//! `Seed` has no `Default` and no ambient constructor anywhere), the
//! algorithm is a NAMED counter-based generator recorded in receipts,
//! stream splits are DECLARED paths whose topology changes the stream
//! while call order never does (parallel == sequential), and a receipt
//! binds (seed, algorithm, stream path) so a stochastic answer is
//! replayable. Distribution semantics are world meanings on another
//! lane; this module owns only the stream primitive they must share.

use emath_core::stochastic::{
    local_stream_seed, stream_value, StochasticReceipt, StreamPath, Seed,
    ALGORITHM_PHILOX4X32_10, E_STOCH_ALGORITHM, E_STOCH_STREAM,
};

/// The stream primitive is a PURE function of (seed, algorithm, path,
/// counter): repeated or reordered queries give identical values, so
/// parallel and sequential execution cannot diverge.
#[test]
fn stream_values_are_pure_under_reordering() {
    let seed = Seed::new(0x5EED_1234_ABCD_EF01);
    let path = StreamPath::new(vec!["campaign".to_string(), "chain-7".to_string()])
        .expect("declared path");
    let sequential: Vec<u64> = (0..8)
        .map(|c| stream_value(&seed, ALGORITHM_PHILOX4X32_10, &path, c).expect("declared algo"))
        .collect();
    // Same queries, reordered (and interleaved with other streams) must
    // produce identical per-counter values.
    let mut reordered: Vec<(u64, u64)> = (0..8)
        .rev()
        .map(|c| {
            let v = stream_value(&seed, ALGORITHM_PHILOX4X32_10, &path, c).expect("declared");
            (c, v)
        })
        .collect();
    reordered.sort();
    let reordered: Vec<u64> = reordered.into_iter().map(|(_, v)| v).collect();
    assert_eq!(sequential, reordered, "call order must not change values");
    // Purity: a single counter re-queried is the same value.
    let again = stream_value(&seed, ALGORITHM_PHILOX4X32_10, &path, 3).expect("declared");
    assert_eq!(again, sequential[3]);
}

/// Split semantics: different declared topologies give DIFFERENT streams
/// (splitting is real), while the same topology always gives the same
/// stream. Labels are ordered: ["a","b"] and ["b","a"] are different
/// paths. This is the parallel-safety property: topology, not call
/// order, is what changes results.
#[test]
fn split_topology_changes_streams_deterministically() {
    let seed = Seed::new(42);
    let root = StreamPath::root();
    let a = root.child("a");
    let ab = a.child("b");
    let ba = StreamPath::new(vec!["b".to_string(), "a".to_string()]).expect("declared");
    let value = |p: &StreamPath, c: u64| {
        stream_value(&seed, ALGORITHM_PHILOX4X32_10, p, c).expect("declared")
    };
    // Splits differ from the root and from each other.
    assert_ne!(value(&root, 0), value(&a, 0), "a split must move the stream");
    assert_ne!(value(&a, 0), value(&ab, 0), "deeper split moves the stream");
    assert_ne!(value(&ab, 0), value(&ba, 0), "labels are ordered");
    // Same topology, same values — across "parallelism levels" (many
    // counters queried in one go vs one at a time).
    let batch: Vec<u64> = (100..104)
        .map(|c| value(&ab, c))
        .collect();
    let singles: Vec<u64> = (100..104)
        .map(|c| {
            let fresh_ab = StreamPath::root().child("a").child("b");
            stream_value(&seed, ALGORITHM_PHILOX4X32_10, &fresh_ab, c).expect("declared")
        })
        .collect();
    assert_eq!(batch, singles, "same seed + same topology = same stream");
    // Seeds are identity: a different seed gives a different stream.
    assert_ne!(
        value(&root, 0),
        stream_value(&Seed::new(43), ALGORITHM_PHILOX4X32_10, &root, 0).expect("declared")
    );
}

/// Algorithm identity is a CLOSED gate: the named counter-based generator
/// is admitted; an unknown name refuses typed (never a silently different
/// generator behind the same receipt).
#[test]
fn algorithm_identity_is_a_closed_gate() {
    let seed = Seed::new(7);
    let path = StreamPath::root();
    assert!(
        stream_value(&seed, "philox4x32-7", &path, 0)
            .unwrap_err()
            .contains(E_STOCH_ALGORITHM),
        "unknown algorithm refuses typed"
    );
    assert!(
        stream_value(&seed, "", &path, 0)
            .unwrap_err()
            .contains(E_STOCH_ALGORITHM),
        "empty algorithm name refuses typed"
    );
    let v = stream_value(&seed, ALGORITHM_PHILOX4X32_10, &path, 0).expect("named algo admits");
    let _ = v;
}

/// A malformed stream path refuses: labels must be non-empty (the empty
/// path itself is the legal ROOT stream).
#[test]
fn stream_paths_validate() {
    assert!(StreamPath::root().canonical().is_empty(), "root = empty path");
    assert_eq!(
        StreamPath::new(vec!["a".to_string(), "".to_string()]).unwrap_err(),
        format!("{E_STOCH_STREAM}: label 1 must be non-empty")
    );
    let path = StreamPath::new(vec!["x".to_string(), "10".to_string()]).expect("declared");
    assert_eq!(path.canonical(), "x.10");
}

/// The local-generator bridge (ONE seed story): a stateful SplitMix-class
/// generator initializes its state from the CONTRACT — counter 0 of the
/// declared stream — never from its own seed namespace.
#[test]
fn local_generator_seed_derives_from_the_contract() {
    let seed = Seed::new(0xABCDEF01_23456789);
    let path = StreamPath::root().child("mcmc-chain-1");
    let s1 = local_stream_seed(&seed, &path).expect("declared algo");
    let s2 = local_stream_seed(&seed, &path).expect("declared algo");
    assert_eq!(s1, s2, "derivation is deterministic");
    // It IS counter 0 of the declared stream — one mapping, no second
    // RNG namespace.
    assert_eq!(
        s1,
        stream_value(&seed, ALGORITHM_PHILOX4X32_10, &path, 0).expect("declared")
    );
    // Path-sensitive: a different declared topology seeds a different
    // local generator.
    assert_ne!(
        s1,
        local_stream_seed(&seed, &path.child("arm-2")).expect("declared")
    );
    // Seed-sensitive: the seed is identity here too.
    assert_ne!(
        s1,
        local_stream_seed(&Seed::new(0xABCDEF01_2345678A), &path).expect("declared")
    );
}

/// Receipts bind (seed, algorithm, stream path): the content id is stable
/// for the same binding, changes when ANY component changes, and the
/// canonical form names all three — a receipt that cannot reproduce its
/// run is not a receipt.
#[test]
fn receipts_bind_seed_algorithm_and_stream() {
    let seed = Seed::new(0xDEADBEEF);
    let path = StreamPath::root().child("replicate-3");
    let r1 = StochasticReceipt::new(&seed, ALGORITHM_PHILOX4X32_10, &path);
    let r2 = StochasticReceipt::new(&seed, ALGORITHM_PHILOX4X32_10, &path);
    assert_eq!(r1.content_id(), r2.content_id(), "same binding = same receipt id");
    let canonical = r1.canonical();
    assert!(canonical.contains(ALGORITHM_PHILOX4X32_10), "{canonical}");
    assert!(canonical.contains(&seed.to_string()), "{canonical}");
    assert!(canonical.contains("replicate-3"), "{canonical}");
    assert!(r1.content_id().starts_with("fnv1a64:"), "{canonical}");
    // Any component change changes the binding.
    let other_seed = StochasticReceipt::new(&Seed::new(0xDEADBEEF + 1), ALGORITHM_PHILOX4X32_10, &path);
    assert_ne!(r1.content_id(), other_seed.content_id());
    let other_path = StochasticReceipt::new(&seed, ALGORITHM_PHILOX4X32_10, &path.child("x"));
    assert_ne!(r1.content_id(), other_path.content_id());
}

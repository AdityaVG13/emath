//! Build-pipeline metric collectors and the stable benchmark receipt.
//!
//! Collectors record what actually happened on the production build path
//! (phase durations, counters); the receipt is the durable JSON form.
//! The receipt *format* is deterministic — fixed schema id, version and
//! sorted keys — while the recorded durations are measurements and vary
//! run to run. Receipts are evidence objects: they never escalate
//! authority and claim nothing beyond the recorded run.

use emath_artifact::JsonWriter;
use std::collections::BTreeMap;

/// JSON `$schema` id of the benchmark receipt document.
pub const BENCHMARK_RECEIPT_SCHEMA: &str = "emath.benchmark-receipt";
/// Benchmark receipt document version.
pub const BENCHMARK_RECEIPT_VERSION: u32 = 1;

/// Accumulates named phase durations and counters for one build run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetricsCollector {
    durations_ns: BTreeMap<String, u64>,
    counts: BTreeMap<String, u64>,
}

impl MetricsCollector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `nanos` to the named phase duration (accumulating, so a
    /// phase entered twice records its total).
    pub fn record_duration_ns(&mut self, phase: &str, nanos: u64) {
        let slot = self.durations_ns.entry(phase.to_string()).or_default();
        *slot = slot.saturating_add(nanos);
    }

    /// Adds `value` to the named counter.
    pub fn record_count(&mut self, counter: &str, value: u64) {
        let slot = self.counts.entry(counter.to_string()).or_default();
        *slot = slot.saturating_add(value);
    }

    /// Renders the stable JSON benchmark receipt. Keys are emitted in
    /// sorted order under `duration_ns.` and `count.` prefixes, so the
    /// same recorded values always produce the same bytes.
    #[must_use]
    pub fn benchmark_receipt(&self, source: &str, artifact_id: &str) -> String {
        let mut object = JsonWriter::object();
        object.string("schema", BENCHMARK_RECEIPT_SCHEMA);
        object.int("version", u64::from(BENCHMARK_RECEIPT_VERSION));
        object.string("source", source);
        object.string("artifact_id", artifact_id);
        for (phase, nanos) in &self.durations_ns {
            object.int(&format!("duration_ns.{phase}"), *nanos);
        }
        for (counter, value) in &self.counts {
            object.int(&format!("count.{counter}"), *value);
        }
        object.finish()
    }
}

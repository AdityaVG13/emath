//! Lab-runtime entry proving the asupersync `Cx` lane (feature-gated).
//!
//! Foundation artifact for the tokio → asupersync cutover (pass 2,
//! `forks/franken/CUTOVER_PLAN.md` §9.6). The default build stays std-only:
//! this module and its dependency only exist behind the `async-runtime`
//! feature. Production server logic is NOT migrated here — the blocking
//! `run()` loop in `lib.rs` is untouched; transport lands in pass 3.

use asupersync::Cx;

/// Runs async code on a current-thread test runtime with a runtime `Cx`.
///
/// Canonical entry per asupersync docs (`test_utils::run_test` + the
/// `LabRuntime` family): deterministic, virtual-time-capable tests stay off
/// the production runtime, and no wall clock is used (anti-pattern 15).
///
/// The `Cx` is resolved from the ambient runtime context (instead of
/// `Cx::for_testing()`, whose handle carries no spawn gateway at this rev —
/// `Cx::spawn` there returns `SpawnError::RuntimeUnavailable`).
pub fn run_with_cx<F, Fut>(f: F)
where
    F: FnOnce(Cx) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    asupersync::test_utils::run_test(move || async move {
        let cx = Cx::current().expect("test runtime Cx must be installed by block_on");
        f(cx).await;
    });
}

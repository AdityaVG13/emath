//! Lab-runtime entry proving the asupersync `Cx` lane (feature-gated).
//!
//! Exists only behind the `async-runtime` feature; the blocking `run()` loop
//! is untouched and the transport lane lands in pass 3.

use asupersync::Cx;

/// Runs async code on a current-thread test runtime with the ambient `Cx`.
///
/// Uses `Cx::current`, not `Cx::for_testing` — that handle has no spawn
/// gateway at this rev (`Cx::spawn` returns `SpawnError::RuntimeUnavailable`).
pub fn run_with_cx<F, Fut>(f: F)
where
    F: FnOnce(Cx) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    asupersync::test_utils::run_test(move || async move {
        // ubs:ignore — test-only helper; run_test installs Cx before the body runs.
        let cx = Cx::current().expect("test runtime Cx must be installed by block_on");
        f(cx).await;
    });
}

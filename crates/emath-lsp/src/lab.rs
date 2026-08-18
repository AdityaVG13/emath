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

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::runtime::JoinError;
    use std::cell::Cell;

    #[test]
    fn region_task_completes() {
        // A region-owned spawn joins back its typed value, proving the
        // `Cx::spawn` / `TaskHandle::join` wiring. Fails if either is broken.
        let observed = Cell::new(None::<String>);
        run_with_cx(|cx| {
            let observed = &observed;
            async move {
                let task = cx.spawn(|task_cx| async move {
                    let _ = task_cx.checkpoint();
                    42
                });
                let mut task = match task {
                    Ok(task) => task,
                    Err(error) => {
                        observed.set(Some(format!("spawn refused: {error}")));
                        return;
                    }
                };
                match task.join(&cx).await {
                    Ok(42) => observed.set(Some("Ok(42)".to_owned())),
                    Ok(other) => observed.set(Some(format!("unexpected value {other}"))),
                    Err(error) => observed.set(Some(format!("join failed: {error:?}"))),
                }
            }
        });
        assert_eq!(observed.into_inner().as_deref(), Some("Ok(42)"));
    }

    #[test]
    fn region_close_cancels_dropped_task() {
        // Region close = quiescence: a region-owned spawn dropped before
        // completion must be cancelled and drained when the region returns.
        // The task never completes on its own, so a leaked task or a lost
        // cancel would hang this test (fail), not pass silently.
        run_with_cx(|cx| async move {
            // The task must acknowledge cancellation (sync checkpoint
            // returns Err(Cancelled) once the region's cancel signal is
            // set); a cancellation-blind task (e.g. `pending()`) never
            // drains and region close would wait on it forever.
            let task = cx.spawn(|task_cx| async move {
                // Cancellation is signalled before this task is polled
                // (abort / region close), so the first checkpoint breaks.
                while task_cx.checkpoint().is_ok() {}
            });
            assert!(task.is_ok(), "spawn must be admitted in a live region");
            drop(task.expect("checked above"));
        });
    }

    #[test]
    fn abort_makes_join_report_cancellation() {
        // A cancelled task must surface `JoinError::Cancelled`, not a fake
        // success: keeps cancellation distinct from ordinary success
        // (anti-pattern 22).
        let observed = Cell::new(None::<String>);
        run_with_cx(|cx| {
            let observed = &observed;
            async move {
                let task = cx.spawn(|task_cx| async move {
                    // Abort is signalled before this task is polled, so
                    // the first checkpoint breaks the loop.
                    while task_cx.checkpoint().is_ok() {}
                });
                let mut task = match task {
                    Ok(task) => task,
                    Err(error) => {
                        observed.set(Some(format!("spawn refused: {error}")));
                        return;
                    }
                };
                task.abort();
                match task.join(&cx).await {
                    Err(JoinError::Cancelled(_)) => observed.set(Some("Cancelled".to_owned())),
                    other => observed.set(Some(format!("expected Cancelled, got {other:?}"))),
                }
            }
        });
        assert_eq!(observed.into_inner().as_deref(), Some("Cancelled"));
    }
}

//! `emath-cli` lsp lab-runtime tests (migrated from
//! `crates/emath-cli/src/lsp/lab.rs`).

use asupersync::runtime::{JoinError, RuntimeBuilder};
use emath_cli::lsp::lab::run_with_cx;
use std::cell::Cell;
use std::time::{Duration, Instant};

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
fn runtime_shutdown_bounds_stray_never_yielding_task() {
    // TaskHandle::drop is DETACH, not cancel: the runtime deliberately
    // keeps fire-and-forget spawns alive (JoinFuture::drop is the
    // aborting variant; TaskHandle::drop is not), so nothing signals the
    // task below and it never yields. That is the exact non-cooperative
    // task that makes plain Runtime::drop join its worker forever
    // (upstream #60). The contract under test: `shutdown_timeout` bounds
    // that teardown — it returns within its bound instead of hanging the
    // process. Regression to an unbounded shutdown hangs this test
    // (fail), matching this file's fail-by-hang convention. Whether the
    // stray task ever got scheduled decides the boolean result, so only
    // the bound is asserted here, not the drain outcome.
    let runtime = RuntimeBuilder::current_thread()
        .build()
        .expect("test runtime builds");
    // Fire-and-forget spawn of work that never yields and never
    // completes: the exact non-cooperative task the comment above
    // describes, spawned through the runtime handle the way any
    // fire-and-forget caller would.
    let handle = runtime.handle().spawn(async {
        loop {
            std::hint::spin_loop();
        }
    });
    drop(handle);
    let started = Instant::now();
    let drained = runtime.shutdown_timeout(Duration::from_millis(500));
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "shutdown_timeout must return promptly even with a never-yielding \
         stray task (took {elapsed:?})"
    );
    // Either outcome is compliant: `true` means the task never got
    // scheduled before shutdown, `false` means a worker is still stuck
    // inside the task's poll and the reaper owns it now. The guarantee
    // under test is the bound, not the drain.
    let _ = drained;
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

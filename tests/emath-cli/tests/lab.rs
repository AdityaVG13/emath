//! `emath-cli` lsp lab-runtime tests.
use asupersync::runtime::{JoinError, RuntimeBuilder};
use emath_cli::lsp::lab::run_with_cx;
use emath_test_harness::Probe;
use std::cell::Cell;
use std::time::{Duration, Instant};
#[test]
fn probe() {
    let mut p = Probe::new("lab region spawns join, aborts cancel, stray tasks stay bounded");
    p.case("join", |p| { let seen = Cell::new(None::<String>); run_with_cx(|cx| { let seen = &seen; async move { let mut t = match cx.spawn(|c| async move { let _ = c.checkpoint(); 42 }) { Ok(t) => t, Err(e) => { seen.set(Some(format!("spawn refused: {e}"))); return; } }; match t.join(&cx).await { Ok(42) => seen.set(Some("Ok(42)".into())), Ok(o) => seen.set(Some(format!("unexpected {o}"))), Err(e) => seen.set(Some(format!("join failed: {e:?}"))) } } }); p.eq("value", seen.into_inner().as_deref(), Some("Ok(42)")); });
    p.case("abort", |p| { let seen = Cell::new(None::<String>); run_with_cx(|cx| { let seen = &seen; async move { let mut t = match cx.spawn(|c| async move { while c.checkpoint().is_ok() {} }) { Ok(t) => t, Err(e) => { seen.set(Some(format!("spawn refused: {e}"))); return; } }; t.abort(); match t.join(&cx).await { Err(JoinError::Cancelled(_)) => seen.set(Some("Cancelled".into())), other => seen.set(Some(format!("expected Cancelled, got {other:?}"))) } } }); p.eq("cancel", seen.into_inner().as_deref(), Some("Cancelled")); });
    p.case("bound", |p| { let rt = RuntimeBuilder::current_thread().build().expect("runtime"); drop(rt.handle().spawn(async { loop { std::hint::spin_loop(); } })); let t = Instant::now(); let _ = rt.shutdown_timeout(Duration::from_millis(500)); p.demand("prompt", t.elapsed() < Duration::from_secs(5), "shutdown_timeout must bound a never-yielding stray"); });
    p.finish();
}

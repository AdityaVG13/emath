// Cooperative artifact execution. Units count generated body entries,
// control iterations and evaluated tree nodes, not VM instructions or time.
use std::cell::{Cell, RefCell};

pub const DEFAULT_WORK: u64 = 50_000_000;
/// The VM's call-depth law (`MAX_CALL_DEPTH`): 10 000 nested authored calls.
pub const DEFAULT_DEPTH: usize = 10_000;
pub const MAX_DEPTH: usize = 10_000;
/// Depth ceiling for a run that is not on the reserved stack (an entry whose
/// carriers cannot cross threads, called from a host thread of unknown size).
/// 512 levels fit a default 2 MiB spawned thread in debug builds.
pub const OFF_STACK_DEPTH: usize = 512;
/// Stack reserved for a top-level native call. Pages are committed on use;
/// debug builds spend about 1 KiB per level on the smallest bodies.
pub const STACK_RESERVE: usize = 512 << 20;
const WORK_FAULT: &str = "E-NATIVE-WORK: native execution work limit exhausted";
const DEPTH_FAULT: &str = "E-NATIVE-DEPTH: native execution depth limit exhausted";
const STACK_FAULT: &str = "E-NATIVE-STACK: could not start the native execution thread";

#[derive(Clone, Copy)]
struct State {
    remaining: u64,
    depth: usize,
    max_depth: usize,
    fault: Option<&'static str>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static ON_STACK: Cell<bool> = const { Cell::new(false) };
}

/// An execution call or dynamic evaluator frame. Drop releases depth on
/// return, refusal or host unwinding; tail loops retain their call frame.
pub struct Frame {
    owner: bool,
    counted: bool,
}

impl Frame {
    /// One authored call: charges work and one level of depth.
    pub fn enter() -> Result<Self, String> {
        Self::open(true)
    }

    /// The public wrapper of a recursive entry: its first `__self` frame is
    /// the authored call, so the wrapper charges work but no depth.
    pub fn enter_entry() -> Result<Self, String> {
        Self::open(false)
    }

    fn open(counted: bool) -> Result<Self, String> {
        let limit_cap = if ON_STACK.with(Cell::get) { MAX_DEPTH } else { OFF_STACK_DEPTH };
        STATE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let owner = slot.is_none();
            let state = slot.get_or_insert(State {
                remaining: DEFAULT_WORK,
                depth: 0,
                max_depth: DEFAULT_DEPTH,
                fault: None,
            });
            let fault = if let Some(fault) = state.fault {
                Some(fault)
            } else if state.remaining == 0 {
                Some(WORK_FAULT)
            } else if counted && state.depth >= state.max_depth.min(limit_cap) {
                Some(DEPTH_FAULT)
            } else {
                state.remaining -= 1;
                state.depth += usize::from(counted);
                None
            };
            if let Some(fault) = fault {
                state.fault = Some(fault);
                if owner {
                    *slot = None;
                }
                Err(fault.into())
            } else {
                Ok(Self { owner, counted })
            }
        })
    }

    /// Exhaustion is sticky: catching a nested error cannot turn it into a
    /// successful native result or replenish the current run.
    pub fn check(&self) -> Result<(), String> {
        check()
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        STATE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if self.owner {
                *slot = None;
            } else if let Some(state) = slot.as_mut() {
                state.depth -= usize::from(self.counted);
            }
        });
    }
}

fn failure() -> Option<&'static str> {
    STATE.with(|slot| slot.borrow().as_ref().and_then(|state| state.fault))
}

pub fn check() -> Result<(), String> {
    failure().map_or(Ok(()), |fault| Err(fault.into()))
}

/// Enter a dynamic execution frame and preserve sticky refusal on return.
pub fn run<T>(body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let frame = Frame::enter()?;
    let result = body();
    frame.check()?;
    result
}

/// Charge iteration work before executing it. Callers already hold a frame.
pub fn charge(units: u64) -> Result<(), String> {
    STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot
            .as_mut()
            .ok_or_else(|| String::from("E-NATIVE-CONTEXT: work charged outside execution"))?;
        if let Some(fault) = state.fault {
            return Err(fault.into());
        }
        if units > state.remaining {
            state.fault = Some(WORK_FAULT);
            return Err(WORK_FAULT.into());
        }
        state.remaining -= units;
        Ok(())
    })
}

struct Scope {
    previous: Option<(u64, usize)>,
    initial: u64,
}

impl Drop for Scope {
    fn drop(&mut self) {
        STATE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if let Some((remaining, max_depth)) = self.previous {
                if let Some(state) = slot.as_mut() {
                    state.remaining = remaining - (self.initial - state.remaining);
                    state.max_depth = max_depth;
                }
            } else {
                *slot = None;
            }
        });
    }
}

/// Bound a synchronous native run on the current thread. Nested scopes can
/// restrict, never increase, the parent's remaining work or depth. Work used
/// inside them stays charged to the parent. The depth ceiling is MAX_DEPTH.
/// Opaque host callbacks are not preempted; they must return cooperatively.
pub fn with_budget<T>(
    work: u64,
    depth: usize,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _scope = STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(state) = slot.as_mut() {
            let previous = Some((state.remaining, state.max_depth));
            state.remaining = state.remaining.min(work);
            state.max_depth = state
                .max_depth
                .min(state.depth.saturating_add(depth.min(MAX_DEPTH)));
            Scope {
                previous,
                initial: state.remaining,
            }
        } else {
            *slot = Some(State {
                remaining: work,
                depth: 0,
                max_depth: depth.min(MAX_DEPTH),
                fault: None,
            });
            Scope {
                previous: None,
                initial: work,
            }
        }
    });
    let result = run();
    failure().map_or(result, |fault| Err(fault.into()))
}

/// Run a top-level native call on a thread with [`STACK_RESERVE`] bytes of
/// stack, so the full [`MAX_DEPTH`] fits in debug builds. The caller's budget
/// state moves to that thread and back, so `with_budget` scopes and sticky
/// faults behave as on one thread. Calls already on the reserved stack run in
/// place; a host that calls entries in a loop can wrap the loop in one
/// `on_stack` to pay for a single thread.
pub fn on_stack<T: Send>(run: impl FnOnce() -> Result<T, String> + Send) -> Result<T, String> {
    if ON_STACK.with(Cell::get) || cfg!(target_family = "wasm") {
        return run();
    }
    let before = STATE.with(|slot| *slot.borrow());
    let joined = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("emath-native".into())
            .stack_size(STACK_RESERVE)
            .spawn_scoped(scope, move || {
                ON_STACK.with(|flag| flag.set(true));
                STATE.with(|slot| *slot.borrow_mut() = before);
                let result = run();
                (result, STATE.with(|slot| slot.borrow_mut().take()))
            })
            .map(|handle| handle.join())
    });
    match joined {
        Ok(Ok((result, after))) => {
            STATE.with(|slot| *slot.borrow_mut() = after);
            result
        }
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => Err(STACK_FAULT.into()),
    }
}

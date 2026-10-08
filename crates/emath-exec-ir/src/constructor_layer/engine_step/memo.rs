use super::super::{Engine, CValue, ConstructorError, fault, cint, Code, call_memo_key, Closure, closure_memo_key, is_scalar, MAX_CALL_DEPTH, ContinuationFrame, Environment, ScalarArg, ScalarArgs, SCALAR_ARGS, FACT_PREFIX};
use super::prelude::{fragment_term, substitute_path, value_to_expr};
use super::super::world::{with_world, world_value, CarrierWorld};
use crate::exact_int::ExactInt;
use super::super::checkpoint::memoizable_value;

/// The arguments as a `ScalarArgs` key, when they all fit one.
fn scalar_args(args: &[CValue]) -> Option<ScalarArgs> {
    if args.len() > SCALAR_ARGS {
        return None;
    }
    let mut key = ScalarArgs { len: args.len() as u8, items: [ScalarArg::Unused; SCALAR_ARGS] };
    for (slot, arg) in key.items.iter_mut().zip(args) {
        *slot = match arg {
            CValue::Bool(b) => ScalarArg::Bool(*b),
            CValue::Int(ExactInt::Small(n)) => ScalarArg::Int(*n),
            CValue::Rat { num: ExactInt::Small(n), den: ExactInt::Small(d) } => ScalarArg::Rat(*n, *d),
            _ => return None,
        };
    }
    Some(key)
}

impl Engine {
    pub(in crate::constructor_layer) fn quote_substitute(
        &mut self,
        fragment: CValue,
        reference: &str,
        replacement: CValue,
    ) -> Result<CValue, ConstructorError> {
        let expr = fragment_term(fragment).map_err(|message| fault("type", message))?;
        let expr = substitute_path(
            &expr,
            reference,
            &value_to_expr(&replacement),
            &mut self.next_ref,
        );
        let deps = self.dependency_snapshot(&expr);
        Ok(CValue::Code(Box::new(Code { expr, deps })))
    }

    pub(in crate::constructor_layer) fn completed_call(
        &self,
        name: &str,
        args: &[CValue],
    ) -> Option<CValue> {
        if let Some(key) = self.scalar_key(name, args) {
            return self.scalar_memo.get(name)?.get(&key).filter(|value| memoizable_value(value)).cloned();
        }
        if self.memo.is_empty() || !self.memoizes(name, args) {
            return None;
        }
        let key = call_memo_key(name, args)?;
        let hit = self.memo.get(&key).filter(|value| memoizable_value(value)).cloned();
        if hit.is_some() && name.starts_with(FACT_PREFIX) {
            self.fact_hits.set(self.fact_hits.get() + 1);
        }
        hit
    }

    pub(in crate::constructor_layer) fn remember_call(
        &mut self,
        name: &str,
        args: &[CValue],
        value: CValue,
    ) {
        if !memoizable_value(&value) {
            return;
        }
        if let Some(key) = self.scalar_key(name, args) {
            let calls = match self.scalar_memo.get_mut(name) {
                Some(calls) => calls,
                None => self.scalar_memo.entry(name.to_string()).or_default(),
            };
            calls.entry(key).or_insert(value);
            return;
        }
        if !self.memoizes(name, args) {
            return;
        }
        if let Some(key) = call_memo_key(name, args) {
            if self.facts_only_memo {
                if !self.memo.contains_key(&key) {
                    self.memo_added.push(key.clone());
                    self.memo.insert(key, value);
                }
            } else {
                self.memo.insert(key, value);
            }
        }
    }

    /// Fact calls are memoized (and carried), and so are calls whose
    /// arguments are all scalars: their keys cost a few bytes, and they
    /// are what naive recursion (`fibonacci(n - 1) + fibonacci(n - 2)`)
    /// and fact bodies (`pool(0)`, `row(i)`) repeat. Calls carrying
    /// sequences or records are not: a fold threading its state would
    /// serialize the whole state into a fresh key at every step.
    fn memoizes(&self, name: &str, args: &[CValue]) -> bool {
        name.starts_with(FACT_PREFIX) || args.iter().all(is_scalar)
    }

    /// A body that tail-calls hands its frame to the callee, so the
    /// caller is remembered with the chain's final value. Under
    /// `facts_only_memo` that is every call `memoizes` keeps; under the
    /// all-calls memo only facts, since a tail loop threading a sequence
    /// would serialize it at every step.
    pub(in crate::constructor_layer) fn remembers_tail_caller(&self, name: &str, args: &[CValue]) -> bool {
        name.starts_with(FACT_PREFIX) || (self.facts_only_memo && self.memoizes(name, args))
    }

    /// The `scalar_memo` key for a non-fact call under `facts_only_memo`.
    /// Fact calls keep text keys: those are what a host carries and
    /// persists. A scalar with a big part also keeps the text key.
    fn scalar_key(&self, name: &str, args: &[CValue]) -> Option<ScalarArgs> {
        if !self.facts_only_memo || name.starts_with(FACT_PREFIX) {
            return None;
        }
        scalar_args(args)
    }

    /// Under `facts_only_memo` only recursive closures are memoized:
    /// naive recursion (`recur f ...: f(n - 1) + f(n - 2)`) is
    /// exponential without it, while a plain lambda applied across an
    /// archive would pay a body-encoding key per application.
    fn memoizes_closure(&self, clos: &Closure) -> bool {
        !self.facts_only_memo || clos.recursive.is_some()
    }

    pub(in crate::constructor_layer) fn completed_closure(
        &self,
        clos: &Closure,
        args: &[CValue],
    ) -> Option<CValue> {
        if self.memo.is_empty() || !self.memoizes_closure(clos) {
            return None;
        }
        let key = closure_memo_key(clos, args)?;
        self.memo.get(&key).filter(|value| memoizable_value(value)).cloned()
    }

    pub(in crate::constructor_layer) fn remember_closure(
        &mut self,
        clos: &Closure,
        args: &[CValue],
        value: CValue,
    ) {
        if !self.memoizes_closure(clos) || !memoizable_value(&value) {
            return;
        }
        if let Some(key) = closure_memo_key(clos, args) {
            if self.facts_only_memo {
                // Lent-in carried store: the key is split back out (and
                // dropped, not being a fact) when the evaluation ends.
                if !self.memo.contains_key(&key) {
                    self.memo_added.push(key.clone());
                    self.memo.insert(key, value);
                }
            } else {
                self.memo.insert(key, value);
            }
        }
    }

    /// `quote.cost(f, x, limit)`: apply `f` to `x` with at most `limit`
    /// engine steps and report `Cost {done, work, value}` (`value` only
    /// when done; a fault, including the limit, gives `done: false` and
    /// the fault code as `fault`). The application starts from an empty memo and its
    /// memo entries are dropped afterwards, so a measurement does not
    /// depend on what ran before it: the same `f` and `x` always report
    /// the same work. When the run's own budget runs out first, the
    /// partial measurement is unwound and `budget_exhausted` propagates;
    /// a resume re-evaluates the enclosing expression, so the
    /// measurement restarts rather than continuing half way.
    ///
    /// `quote.world(w, f, x, limit)` is the same measurement under carrier
    /// world `w`: `x` is read as `w` reads data, `f` runs with `w`
    /// installed (the outer world resumes after, also on a fault), and the
    /// record is `WorldAnswer {world, done, work, value | fault}`. The
    /// empty memo matters twice here: no answer computed under one world
    /// is reused under another. Values `f` captured were made in the outer
    /// world and enter as they are, like the program's constants.
    pub(in crate::constructor_layer) fn measure_cost(
        &mut self,
        callee: CValue,
        input: CValue,
        limit: u64,
        world: Option<CarrierWorld>,
    ) -> Result<CValue, ConstructorError> {
        let input = match world {
            Some(world) => world_value(world, input)?,
            None => input,
        };
        let outer_limit = self.work_limit;
        let start = self.work;
        let memo = std::mem::take(&mut self.memo);
        let scalar_memo = std::mem::take(&mut self.scalar_memo);
        let memo_added = self.memo_added.len();
        let frames = self.frames.len();
        let kont = self.frames.last().map_or(0, |frame| frame.kont.len());
        let depth = self.call_depth;
        let env = self.env.clone();
        // The memo policy changes the work (a repeated pure call is free
        // under one and paid under the other), so every host measures
        // under the carrying host's policy.
        let facts_only = std::mem::replace(&mut self.facts_only_memo, true);
        self.work_limit = outer_limit.min(start.saturating_add(limit));
        let outcome = match world {
            Some(world) => with_world(world, || self.apply_value(callee, &[input])),
            None => self.apply_value(callee, &[input]),
        };
        let work = self.work - start;
        let outer_exhausted = self.work > outer_limit;
        self.facts_only_memo = facts_only;
        self.work_limit = outer_limit;
        self.memo = memo;
        self.scalar_memo = scalar_memo;
        self.memo_added.truncate(memo_added);
        self.frames.truncate(frames);
        if let Some(frame) = self.frames.last_mut() {
            frame.kont.truncate(kont);
        }
        self.call_depth = depth;
        self.env = env;
        let mut fields = std::collections::BTreeMap::new();
        match outcome {
            Ok(value) => {
                fields.insert("done".into(), CValue::Bool(true));
                fields.insert("work".into(), cint(work as i64));
                fields.insert("value".into(), value);
            }
            Err(err) if err.code == "budget_exhausted" && outer_exhausted => return Err(err),
            // A candidate that faults is a measured failure, not a run
            // failure: a search over variants must survive a bad one.
            Err(err) => {
                fields.insert("done".into(), CValue::Bool(false));
                fields.insert("work".into(), cint(work.min(limit) as i64));
                fields.insert("fault".into(), CValue::Str(err.code));
            }
        }
        let type_name = match world {
            Some(world) => {
                fields.insert("world".into(), CValue::Str(world.name().into()));
                "WorldAnswer"
            }
            None => "Cost",
        };
        Ok(CValue::Record { type_name: type_name.into(), fields: std::sync::Arc::new(fields) })
    }

    /// One honesty site for the call-depth bound: names the reached
    /// depth, the configured limit, and the attempted frame.
    pub(in crate::constructor_layer) fn recursion_depth_exceeded(
        &self,
        frame: &str,
    ) -> ConstructorError {
        fault(
            "recursion_depth_exceeded",
            format!(
                "call depth {} reached the configured limit {} attempting `{frame}`",
                self.call_depth, MAX_CALL_DEPTH,
            ),
        )
    }

    pub(in crate::constructor_layer) fn push_frame(&mut self, function: &str, kind: super::super::ContinuationKind) {
        // A new frame's body starts by evaluating against the INHERITED
        // environment (machine bodies, argument evaluation before the
        // first definition boundary), so the resume env is the env at the
        // push. Frames that rebind (the named def loop, the closure
        // chain) re-pin at `set_frame_next` right after; machine frames
        // never rebind and keep this pin for their whole body.
        let inherited = if self.facts_only_memo {
            Environment::default()
        } else {
            self.env.clone()
        };
        self.frames.push(ContinuationFrame {
            function: function.to_string(),
            kind,
            pc: self.visit as u64,
            next: String::new(),
            env: inherited,
            kont: Vec::new(),
        });
    }

    // Frame environments exist for resume. Each resume path needs a
    // different env, and the pins place exactly those:
    //
    // - def restart (konts empty): the env at the def boundary, pinned
    //   here. A release inside the interrupted def re-fires on the
    //   restart, so the boundary env is the right one to replay from.
    // - kont resume (the deepest pending work re-evaluates): the env at
    //   the deepest swap, pinned at `eval_scoped`/`eval_code` entry.
    // - drain re-suspension: the env the drained konts left, pinned by
    //   `pin_innermost_env` on the resume error path.
    //
    // A per-step snapshot (the old `refresh_frame` sync) would keep
    // every binding alive across the whole step, and an append onto a
    // bound sequence would copy the whole list instead of happening in
    // place. An evaluation under `facts_only_memo` never suspends and
    // keeps no frames for resume.
    pub(in crate::constructor_layer) fn set_frame_next(&mut self, next: &str) {
        // `next`, `pc`, and the boundary env are resume positions; the
        // env is read only by `snapshot`, which runs after a suspension.
        if self.facts_only_memo {
            return;
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.next = next.to_string();
            frame.pc = self.visit as u64;
            frame.env = self.env.clone();
        }
    }

    pub(in crate::constructor_layer) fn refresh_frame(&mut self) {
        if self.facts_only_memo {
            return;
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.pc = self.visit as u64;
        }
    }

    /// Pin the current frame's env after a MID-DEFINITION binding insert
    /// (`open`/`bind`/match-arm binders) whose body evaluates in this same
    /// frame: a suspension inside that body resumes by re-evaluating the
    /// pending operation, which reads the just-bound name. The def
    /// boundary pin predates the insert, so without this pin the resume
    /// would find the name unbound.
    pub(in crate::constructor_layer) fn pin_frame_env(&mut self) {
        if self.facts_only_memo {
            return;
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.env = self.env.clone();
        }
    }

    pub(in crate::constructor_layer) fn pop_frame(&mut self) {
        self.frames.pop();
    }
}

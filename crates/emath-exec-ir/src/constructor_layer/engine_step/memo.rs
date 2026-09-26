use super::super::{Engine, CValue, ConstructorError, fault, Code, call_memo_key, Closure, closure_memo_key, MAX_CALL_DEPTH, ContinuationFrame, Environment};
use super::prelude::{fragment_term, substitute_path, value_to_expr};

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
        if self.memo.is_empty() || !self.memoizes(name, args) {
            return None;
        }
        let key = call_memo_key(name, args)?;
        let hit = self.memo.get(&key).cloned();
        if hit.is_some() && name.starts_with(super::super::FACT_PREFIX) {
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

    /// Under `facts_only_memo`, fact calls are memoized (and carried),
    /// and so are calls whose arguments are all scalars: their keys cost
    /// a few bytes, and a fact body re-reads the same tables (`pool(0)`,
    /// `row(i)`) on every call. Calls carrying sequences or records are
    /// not, since their key would serialize a whole archive.
    fn memoizes(&self, name: &str, args: &[CValue]) -> bool {
        !self.facts_only_memo
            || name.starts_with(super::super::FACT_PREFIX)
            || args.iter().all(|arg| matches!(arg, CValue::Int(_) | CValue::Rat { .. } | CValue::Bool(_)))
    }

    /// A body that tail-calls hands its frame to the callee, so the
    /// caller is remembered with the chain's final value. Under
    /// `facts_only_memo` that is every call `memoizes` keeps; under the
    /// all-calls memo only facts, since a tail loop threading a sequence
    /// would serialize it at every step.
    pub(in crate::constructor_layer) fn remembers_tail_caller(&self, name: &str, args: &[CValue]) -> bool {
        name.starts_with(super::super::FACT_PREFIX) || (self.facts_only_memo && self.memoizes(name, args))
    }

    pub(in crate::constructor_layer) fn completed_closure(
        &self,
        clos: &Closure,
        args: &[CValue],
    ) -> Option<CValue> {
        if self.memo.is_empty() || self.facts_only_memo {
            return None;
        }
        let key = closure_memo_key(clos, args)?;
        self.memo.get(&key).cloned()
    }

    pub(in crate::constructor_layer) fn remember_closure(
        &mut self,
        clos: &Closure,
        args: &[CValue],
        value: CValue,
    ) {
        if self.facts_only_memo {
            return;
        }
        if let Some(key) = closure_memo_key(clos, args) {
            self.memo.insert(key, value);
        }
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

    pub(in crate::constructor_layer) fn push_frame(&mut self, function: &str) {
        self.frames.push(ContinuationFrame {
            function: function.to_string(),
            pc: self.visit as u64,
            next: String::new(),
            env: Environment::default(),
            kont: Vec::new(),
        });
    }

    // Frame environments exist for `snapshot` (suspend and resume). An
    // evaluation under `facts_only_memo` never suspends, and a snapshot
    // taken here would make the next binding copy the whole environment
    // (it is copy-on-write), so those evaluations skip it.
    pub(in crate::constructor_layer) fn set_frame_next(&mut self, next: &str) {
        let snapshot = !self.facts_only_memo;
        if let Some(frame) = self.frames.last_mut() {
            frame.next = next.to_string();
            frame.pc = self.visit as u64;
            if snapshot {
                frame.env = self.env.clone();
            }
        }
    }

    pub(in crate::constructor_layer) fn refresh_frame(&mut self) {
        let snapshot = !self.facts_only_memo;
        if let Some(frame) = self.frames.last_mut() {
            frame.pc = self.visit as u64;
            if snapshot {
                frame.env = self.env.clone();
            }
        }
    }

    pub(in crate::constructor_layer) fn pop_frame(&mut self) {
        self.frames.pop();
    }
}

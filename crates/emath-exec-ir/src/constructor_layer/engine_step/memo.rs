use super::super::*;
use super::prelude::{fragment_term, substitute_path, value_to_expr};

impl Engine {
    pub(in crate::constructor_layer) fn quote_substitute(
        &mut self,
        fragment: CValue,
        reference: &str,
        replacement: CValue,
    ) -> Result<CValue, ConstructorError> {
        let expr = fragment_term(fragment).map_err(|message| fault("type", message))?;
        let expr = substitute_path(&expr, reference, &value_to_expr(&replacement), &mut self.next_ref);
        let deps = self.dependency_snapshot(&expr);
        Ok(CValue::Code(Box::new(Code { expr, deps })))
    }

    pub(in crate::constructor_layer) fn completed_call(&self, name: &str, args: &[CValue]) -> Option<CValue> {
        let key = call_memo_key(name, args)?;
        self.memo.get(&key).cloned()
    }

    pub(in crate::constructor_layer) fn remember_call(&mut self, name: &str, args: &[CValue], value: CValue) {
        if let Some(key) = call_memo_key(name, args) {
            self.memo.insert(key, value);
        }
    }

    pub(in crate::constructor_layer) fn completed_closure(&self, clos: &Closure, args: &[CValue]) -> Option<CValue> {
        let key = closure_memo_key(clos, args)?;
        self.memo.get(&key).cloned()
    }

    pub(in crate::constructor_layer) fn remember_closure(&mut self, clos: &Closure, args: &[CValue], value: CValue) {
        if let Some(key) = closure_memo_key(clos, args) {
            self.memo.insert(key, value);
        }
    }

    /// One honesty site for the call-depth bound: names the reached
    /// depth, the configured limit, and the attempted frame.
    pub(in crate::constructor_layer) fn recursion_depth_exceeded(&self, frame: &str) -> ConstructorError {
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
            env: BTreeMap::new(),
            kont: Vec::new(),
        });
    }

    pub(in crate::constructor_layer) fn set_frame_next(&mut self, next: &str) {
        if let Some(frame) = self.frames.last_mut() {
            frame.next = next.to_string();
            frame.pc = self.visit as u64;
            frame.env = self.env.clone();
        }
    }

    pub(in crate::constructor_layer) fn refresh_frame(&mut self) {
        if let Some(frame) = self.frames.last_mut() {
            frame.pc = self.visit as u64;
            frame.env = self.env.clone();
        }
    }

    pub(in crate::constructor_layer) fn pop_frame(&mut self) {
        self.frames.pop();
    }

}

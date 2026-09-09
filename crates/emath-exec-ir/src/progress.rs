//! Scoped continuation of authored methods. Saved arguments bind a certificate
//! to its original problem. Checkers execute without replaying refinement.
use std::{cell::RefCell, collections::BTreeMap};
use crate::{EmirOp, EmirProgram, EmirValue, interp::{EvalFault, Value}, native_kernel::{self, MethodContract}};

#[derive(Clone, Debug, PartialEq)]
pub struct MethodFrame {
    pub key: String,
    pub capability: String,
    pub arguments: Vec<Value>,
    pub state: Value,
    pub fault: Option<String>,
}
struct Context {
    previous: BTreeMap<String, MethodFrame>,
    seen: BTreeMap<String, MethodFrame>,
    path: Vec<(emath_core::Span, usize)>,
    advance: Option<String>,
}
thread_local! { static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) }; }
struct Restore(Option<Context>);
impl Drop for Restore {
    fn drop(&mut self) { CONTEXT.with(|slot| *slot.borrow_mut() = self.0.take()); }
}

pub fn with_frames<T>(previous: &[MethodFrame], advance: bool, run: impl FnOnce() -> T) -> (T, Vec<MethodFrame>) {
    let old = CONTEXT.with(|slot| slot.replace(Some(Context {
        previous: previous.iter().map(|frame| (frame.key.clone(), frame.clone())).collect(),
        seen: BTreeMap::new(), path: Vec::new(),
        advance: advance.then(|| previous.iter().filter(|frame| complete(&frame.state) != Some(true)).min_by(|left, right| left.key.cmp(&right.key)).map(|frame| frame.key.clone())).flatten(),
    })));
    let _restore = Restore(old);
    let result = run();
    let frames = CONTEXT.with(|slot| slot.borrow_mut().as_mut().map(|context| std::mem::take(&mut context.seen).into_values().collect()).unwrap_or_default());
    (result, frames)
}

pub(crate) struct Site(bool);
impl Drop for Site {
    fn drop(&mut self) {
        if self.0 { CONTEXT.with(|slot| { if let Some(context) = slot.borrow_mut().as_mut() { context.path.pop(); } }); }
    }
}
pub(crate) fn site(span: emath_core::Span, index: usize) -> Site {
    Site(CONTEXT.with(|slot| {
        if let Some(context) = slot.borrow_mut().as_mut() { context.path.push((span, index)); true } else { false }
    }))
}

pub fn complete(value: &Value) -> Option<bool> { native_kernel::method_complete(value) }

fn invoke(capability: &str, arguments: &[Value]) -> Result<Value, String> {
    let old = CONTEXT.with(|slot| slot.replace(None));
    let _restore = Restore(old);
    let count = u16::try_from(arguments.len()).map_err(|_| "E-METHOD-ABI: too many arguments")?;
    let mut ops: Vec<_> = (0..count).map(|index| (EmirOp::LoadInput(index), emath_core::Span::default())).collect();
    ops.push((EmirOp::ApplyCapability {
        capability: capability.into(), class: emath_ir::CellClass::Pure,
        args: (0..u32::from(count)).map(EmirValue).collect(),
    }, emath_core::Span::default()));
    let program = EmirProgram { ops, result: EmirValue(u32::from(count)), input_count: count, state_count: 0, domain_obligations: Vec::new() };
    crate::interp::evaluate(&program, arguments, &[]).map_err(|error| error.to_string())
}

fn binds(contract: &MethodContract, state: &Value, arguments: &[Value]) -> bool {
    let Value::Record { type_name, fields } = state else { return false; };
    type_name == &contract.record && contract.bindings.iter().all(|(field, (argument, path))| {
        let mut value = arguments.get(*argument);
        for member in path {
            value = match value { Some(Value::Record { fields, .. }) => fields.get(member), _ => None };
        }
        value.zip(fields.get(field)).is_some_and(|(argument, actual)| argument == actual)
    })
}

pub fn validate(frame: &MethodFrame) -> Result<(), String> {
    let contract = native_kernel::installed_method_contract(&frame.capability).ok_or("E-METHOD-BINDING: saved method is not installed")?;
    if complete(&frame.state).is_none() || !binds(&contract, &frame.state, &frame.arguments) {
        return Err("E-METHOD-TARGET: saved state changes the original mathematical problem".into());
    }
    if invoke(&contract.check, std::slice::from_ref(&frame.state))? != Value::Bool(true) {
        return Err("E-CERT-INVALID: saved mathematical certificate failed".into());
    }
    Ok(())
}

pub(crate) fn apply(
    capability: &str,
    arguments: &[Value],
    run: impl FnOnce() -> Result<Value, EvalFault>,
) -> Result<Value, EvalFault> {
    if !CONTEXT.with(|slot| slot.borrow().is_some()) { return run(); }
    let Some(contract) = native_kernel::installed_method_contract(capability) else { return run(); };
    let refuse = |detail: String| EvalFault::CarrierRefused { op: "apply-capability", detail };
    let (key, advance) = CONTEXT.with(|slot| {
        let slot = slot.borrow();
        let context = slot.as_ref().expect("active method scope");
        let key = emath_core::content_id_of_str(&format!("{capability}:{:?}", context.path)).0;
        let advance = context.advance.as_deref() == Some(key.as_str());
        (key, advance)
    });
    let previous = CONTEXT.with(|slot| slot.borrow().as_ref().and_then(|context| context.previous.get(&key).cloned()));
    let mut fault = None;
    let state = if let Some(previous) = previous {
        if previous.capability != capability || previous.arguments != arguments {
            return Err(refuse("E-METHOD-TARGET: saved call arguments changed".into()));
        }
        validate(&previous).map_err(refuse)?;
        if advance && complete(&previous.state) == Some(false) {
            match invoke(&contract.step, &[previous.state.clone(), Value::I64(1)]) {
                Ok(value) => value,
                Err(error) => { fault = Some(error); previous.state }
            }
        } else { fault = previous.fault; previous.state }
    } else { run()? };
    let frame = MethodFrame { key: key.clone(), capability: capability.into(), arguments: arguments.to_vec(), state: state.clone(), fault: fault.clone() };
    validate(&frame).map_err(refuse)?;
    CONTEXT.with(|slot| {
        if let Some(context) = slot.borrow_mut().as_mut() {
            // A method which consumes an earlier state replaces that state's
            // continuation. Pure projections keep their underlying method.
            context.seen.retain(|_, frame| !arguments.iter().any(|argument| argument == &frame.state));
            context.seen.insert(key, frame);
        }
    });
    if let Some(error) = fault { Err(refuse(error)) } else { Ok(state) }
}

//! The seam a built artifact uses to run one function in the reference
//! engine. `quote.cost`, `quote.world`, and `quote.worlds` have no
//! compiled meaning (compiled code has no engine steps and one carrier),
//! so `emath build` emits a function that uses them as a call into this
//! engine: its declared inputs cross in, its declared outputs cross back.
//! Answers, `work` counts, and refusals are the VM's own.
//!
//! The crossing covers the declared carriers an entry signature can name
//! natively: `Int` (`i64`), `Rat` (`(i128, i128)`), `Bool`, `Float64`,
//! `Str`, and sequences (a native tuple is a sequence). A value that does not fit its native
//! carrier refuses by name (`type: ...`), never truncates.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use emath_core::tree::SyntaxTree;

use super::{evaluate_function_at, with_worlds, CValue, CarrierWorld};
use crate::exact_int::ExactInt;

/// A native value the artifact passes into the engine.
pub trait ToCValue {
    fn to_cvalue(&self) -> CValue;
}

/// A native value the artifact reads back from the engine.
pub trait FromCValue: Sized {
    fn from_cvalue(value: &CValue) -> Result<Self, String>;
}

fn mismatch<T>(carrier: &str, value: &CValue) -> Result<T, String> {
    Err(format!("type: the engine answered {value}, which is not a native {carrier}"))
}

impl<T: ToCValue + ?Sized> ToCValue for &T {
    fn to_cvalue(&self) -> CValue {
        (**self).to_cvalue()
    }
}

impl ToCValue for i64 {
    fn to_cvalue(&self) -> CValue {
        CValue::Int(ExactInt::from(*self))
    }
}

impl FromCValue for i64 {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Int(n) => n
                .to_i64()
                .ok_or_else(|| format!("overflow: {n} exceeds the native Int carrier")),
            other => mismatch("Int", other),
        }
    }
}

impl ToCValue for (i128, i128) {
    fn to_cvalue(&self) -> CValue {
        CValue::Rat {
            num: ExactInt::from(self.0),
            den: ExactInt::from(self.1),
        }
    }
}

impl FromCValue for (i128, i128) {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            // A Rat slot admits an Int exactly, as the VM's output check does.
            CValue::Int(n) => n
                .to_i128()
                .map(|n| (n, 1))
                .ok_or_else(|| format!("overflow: {n} exceeds the native Rat carrier")),
            CValue::Rat { num, den } => match (num.to_i128(), den.to_i128()) {
                (Some(num), Some(den)) => Ok((num, den)),
                _ => Err(format!("overflow: {num}/{den} exceeds the native Rat carrier")),
            },
            other => mismatch("Rat", other),
        }
    }
}

impl ToCValue for bool {
    fn to_cvalue(&self) -> CValue {
        CValue::Bool(*self)
    }
}

impl FromCValue for bool {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Bool(b) => Ok(*b),
            other => mismatch("Bool", other),
        }
    }
}

impl ToCValue for f64 {
    fn to_cvalue(&self) -> CValue {
        CValue::Float64(*self)
    }
}

impl FromCValue for f64 {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Float64(x) => Ok(*x),
            other => mismatch("Float64", other),
        }
    }
}

impl ToCValue for str {
    fn to_cvalue(&self) -> CValue {
        CValue::Str(self.to_string())
    }
}

impl ToCValue for String {
    fn to_cvalue(&self) -> CValue {
        CValue::Str(self.clone())
    }
}

impl FromCValue for String {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Str(text) => Ok(text.clone()),
            other => mismatch("Str", other),
        }
    }
}

impl<T: ToCValue> ToCValue for [T] {
    fn to_cvalue(&self) -> CValue {
        CValue::Sequence(Arc::new(self.iter().map(ToCValue::to_cvalue).collect()))
    }
}

impl<T: ToCValue> ToCValue for Vec<T> {
    fn to_cvalue(&self) -> CValue {
        self.as_slice().to_cvalue()
    }
}

impl<T: FromCValue> FromCValue for Vec<T> {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Sequence(items) => items.iter().map(T::from_cvalue).collect(),
            // A native tuple is a sequence of its joined element carrier.
            CValue::Tuple(items) => items.iter().map(T::from_cvalue).collect(),
            other => mismatch("sequence", other),
        }
    }
}

/// One field of a record answer (a multi-output function packs its
/// outputs as a record named after it).
pub fn field<T: FromCValue>(value: &CValue, name: &str) -> Result<T, String> {
    match value {
        CValue::Record { fields, .. } => match fields.get(name) {
            Some(field) => T::from_cvalue(field),
            None => Err(format!("type: the engine's answer has no field `{name}`")),
        },
        other => mismatch("record", other),
    }
}

/// Read a whole answer as a native value.
pub fn read<T: FromCValue>(value: &CValue) -> Result<T, String> {
    T::from_cvalue(value)
}

thread_local! {
    static PARSED: RefCell<BTreeMap<String, Rc<SyntaxTree>>> = RefCell::new(BTreeMap::new());
}

/// Run `name` from the module text `source` in the reference engine
/// under the world list `worlds` (what `quote.worlds()` returns).
/// `path` is where the module was built from: its `use` imports resolve
/// from there, as `emath run` resolves them. The parse is cached per
/// thread and path.
pub fn run_entry(
    source: &str,
    path: &str,
    name: &str,
    inputs: Vec<(&str, CValue)>,
    worlds: &[&str],
) -> Result<CValue, String> {
    let tree = PARSED.with(|cache| {
        Rc::clone(
            cache
                .borrow_mut()
                .entry(path.to_string())
                .or_insert_with(|| Rc::new(emath_syntax::parse_str(source).0)),
        )
    });
    let worlds = worlds
        .iter()
        .map(|world| {
            CarrierWorld::parse(world).ok_or_else(|| format!("unknown_world: unknown world `{world}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let inputs: BTreeMap<String, CValue> = inputs
        .into_iter()
        .map(|(input, value)| (input.to_string(), value))
        .collect();
    with_worlds(&worlds, || evaluate_function_at(&tree, name, &inputs, Some(Path::new(path))))
        .map_err(|error| format!("{}: {}", error.code, error.message))
}

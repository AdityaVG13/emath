// The artifact-side Code value: a quoted unary program compiled once
// into a closure factory. `substitute` binds one open constant by
// partial application; `evaluate` demands closure over the open set
// (open code refuses `unbound_code`, naming every remaining open
// constant). No tree is carried and no interpreter runs: candidates
// execute as the compiled closures the backend emitted.
//
// The carrier is generic: the backend instantiates `Code<V>` over the
// template's declared scalar domain (`Code<ExactRatio>` for Rat,
// `Code<i64>` for Int, `Code<bool>` for Bool). The carrier governs the
// parameter and the open constants together - a quoted Int program is
// an Int program - so the compiled factory stays monomorphic and
// cross-lane parity holds on the admitted subset.
//
// Carrier contract: `free` is the binding order the factory consumes;
// `substitute` removes the bound name from `free` and splices the
// value at its position when forwarding; binding a name that is not
// open is a no-op (tree-substitution parity: substituting an absent
// reference leaves the code unchanged); `evaluate` refuses while any
// name remains open, naming them all in binding order.

use std::rc::Rc;

/// The specialized unary program: a function over the carrier.
pub type Unary<V> = Rc<dyn Fn(V) -> Result<V, String>>;

/// The template factory: consumes the open constants' values (in
/// `free` order) and yields the specialized unary program.
pub type CodeFactory<V> = Rc<dyn Fn(&[V]) -> Result<Unary<V>, String>>;

/// A quoted program template with its open constants.
pub struct Code<V: Clone + 'static> {
    free: Vec<String>,
    make: CodeFactory<V>,
}

impl<V: Clone + 'static> Clone for Code<V> {
    fn clone(&self) -> Self {
        Code {
            free: self.free.clone(),
            make: Rc::clone(&self.make),
        }
    }
}

/// Open a compiled template: `free` names the open constants in the
/// order the factory consumes their values.
pub fn open<V: Clone + 'static>(free: Vec<String>, make: CodeFactory<V>) -> Code<V> {
    Code { free, make }
}

/// Bind one open constant by partial application.
pub fn substitute<V: Clone + 'static>(code: &Code<V>, reference: &str, value: V) -> Code<V> {
    let Some(position) = code.free.iter().position(|name| name == reference) else {
        return code.clone();
    };
    let mut free = code.free.clone();
    free.remove(position);
    let inner = Rc::clone(&code.make);
    let make = Rc::new(move |rest: &[V]| {
        let mut all = rest[..position].to_vec();
        all.push(value.clone());
        all.extend_from_slice(&rest[position..]);
        inner(&all)
    });
    Code { free, make }
}

/// The guarded executor: closed code yields the specialized unary
/// program; open code refuses, naming every remaining constant.
pub fn evaluate<V: Clone + 'static>(code: &Code<V>) -> Result<Unary<V>, String> {
    if !code.free.is_empty() {
        return Err(format!(
            "unbound_code: quoted code is open; unbound name(s): {}",
            code.free.join(", ")
        ));
    }
    (code.make)(&[])
}

/// The open constant names in binding order.
pub fn free_names<V: Clone + 'static>(code: &Code<V>) -> &[String] {
    &code.free
}

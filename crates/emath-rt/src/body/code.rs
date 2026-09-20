// The artifact-side Code value: a quoted unary `Rat -> Rat` program
// compiled once into a closure factory. `substitute` binds one open
// constant by partial application; `evaluate` demands closure over
// the open set (open code refuses `unbound_code`, naming every
// remaining open constant). No tree is carried and no interpreter
// runs: candidates execute as the compiled closures the backend
// emitted.
//
// Carrier contract: `free` is the binding order the factory consumes;
// `substitute` removes the bound name from `free` and splices the
// value at its position when forwarding; binding a name that is not
// open is a no-op (tree-substitution parity: substituting an absent
// reference leaves the code unchanged); `evaluate` refuses while any
// name remains open, naming them all in binding order.

use std::rc::Rc;

use super::ExactRatio;

/// The specialized unary program: an exact-rational function.
pub type UnaryRat = Rc<dyn Fn(ExactRatio) -> Result<ExactRatio, String>>;

/// The template factory: consumes the open constants' values (in
/// `free` order) and yields the specialized unary program.
pub type CodeFactory = Rc<dyn Fn(&[ExactRatio]) -> Result<UnaryRat, String>>;

/// A quoted program template with its open constants.
#[derive(Clone)]
pub struct Code {
    free: Vec<String>,
    make: CodeFactory,
}

/// Open a compiled template: `free` names the open constants in the
/// order the factory consumes their values.
pub fn open(free: Vec<String>, make: CodeFactory) -> Code {
    Code { free, make }
}

/// Bind one open constant by partial application.
pub fn substitute(code: &Code, reference: &str, value: ExactRatio) -> Code {
    let Some(position) = code.free.iter().position(|name| name == reference) else {
        return code.clone();
    };
    let mut free = code.free.clone();
    free.remove(position);
    let inner = Rc::clone(&code.make);
    let make = Rc::new(move |rest: &[ExactRatio]| {
        let mut all = rest[..position].to_vec();
        all.push(value);
        all.extend_from_slice(&rest[position..]);
        inner(&all)
    });
    Code { free, make }
}

/// The guarded executor: closed code yields the specialized unary
/// program; open code refuses, naming every remaining constant.
pub fn evaluate(code: &Code) -> Result<UnaryRat, String> {
    if !code.free.is_empty() {
        return Err(format!(
            "unbound_code: quoted code is open; unbound name(s): {}",
            code.free.join(", ")
        ));
    }
    (code.make)(&[])
}

/// The open constant names in binding order.
pub fn free_names(code: &Code) -> &[String] {
    &code.free
}

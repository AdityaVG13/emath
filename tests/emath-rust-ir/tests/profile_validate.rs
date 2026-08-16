//! Profile-validation witnesses: no_std refuses unsafe code
//! exactly like every other profile (E-CODEGEN-002).

use emath_rust_ir::ast::{Block, FnDef, Item, Module, Stmt, Ty, Visibility};
use emath_rust_ir::{CrateProfile, ProfileProblem};

fn unsafe_fn(name: &str) -> Item {
    Item::Fn(FnDef {
        name: name.to_string(),
        generics: vec![],
        params: vec![],
        ret: Ty::Unit,
        body: Stmt::Block(Block::default()),
        doc: vec![],
        visibility: Visibility::Public,
        attrs: vec!["unsafe".to_string()],
    })
}

#[test]
fn no_std_profile_refuses_unsafe() {
    let module = Module {
        items: vec![unsafe_fn("bad_ffi")],
    };
    let problems = CrateProfile::NoStd.validate(&module);
    assert_eq!(
        problems,
        vec![ProfileProblem::UnsafeInSafeProfile("fn bad_ffi".into())]
    );
}

#[test]
fn library_profile_refuses_unsafe() {
    let module = Module {
        items: vec![unsafe_fn("bad_ffi")],
    };
    let problems = CrateProfile::Library.validate(&module);
    assert_eq!(
        problems,
        vec![ProfileProblem::UnsafeInSafeProfile("fn bad_ffi".into())]
    );
}

#[test]
fn clean_module_validates_without_problems() {
    let module = Module { items: vec![] };
    assert!(CrateProfile::NoStd.validate(&module).is_empty());
    assert!(CrateProfile::Library.validate(&module).is_empty());
}

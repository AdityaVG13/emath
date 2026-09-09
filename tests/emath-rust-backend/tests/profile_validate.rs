//! Profile-validation witnesses: no_std refuses unsafe code
//! exactly like every other profile (E-CODEGEN-002).

use emath_rust_backend::rust_ir::ast::{Block, FnDef, Item, Module, Stmt, Ty, Visibility};
use emath_rust_backend::rust_ir::{CrateProfile, ProfileProblem};
use emath_test_harness::Probe;

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
fn profile_validate() {
    let mut p = Probe::new("safe profiles refuse unsafe code with E-CODEGEN-002");
    for profile in [CrateProfile::NoStd, CrateProfile::Library] {
        p.case(profile.name(), |p| {
            p.eq(
                "unsafe",
                profile.validate(&Module { items: vec![unsafe_fn("bad_ffi")] }),
                vec![ProfileProblem::UnsafeInSafeProfile("fn bad_ffi".into())],
            );
            p.demand("clean", profile.validate(&Module { items: vec![] }).is_empty(), "clean module validates");
        });
    }
    p.finish();
}

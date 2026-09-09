//! Builtin worlds cover at least five classes with deterministic distinct identities.

use emath_test_harness::Probe;
use emath_world_ir::builtin::{WorldClass, builtin_worlds};

#[test]
fn builtin() {
    let mut p = Probe::new("builtin worlds cover five classes with stable distinct identities");
    let worlds = builtin_worlds();
    p.demand("five", worlds.len() >= 5, "at least five world classes");
    p.eq("count", worlds.len(), WorldClass::ALL.len());
    p.case("identities", |p| {
        let mut seen = std::collections::BTreeMap::new();
        for world in &worlds {
            p.eq(format!("stable/{}", world.world.name), world.identity(), world.identity());
            if let Some(previous) = seen.insert(world.identity(), world.class) {
                p.fail("distinct", format!("collision between {previous:?} and {:?}", world.class));
            } else {
                p.demand(format!("distinct/{}", world.world.name), true, "distinct");
            }
        }
        p.eq("order", worlds.iter().map(|w| w.class).collect::<Vec<_>>(), WorldClass::ALL.to_vec());
    });
    p.case("rebuild", |p| {
        let second = builtin_worlds();
        p.eq("equal", worlds.clone(), second.clone());
        for (a, b) in worlds.iter().zip(&second) {
            p.eq(format!("canonical/{}", a.world.name), a.world.canonical(), b.world.canonical());
            p.eq(format!("identity/{}", a.world.name), a.identity(), b.identity());
        }
    });
    p.finish();
}

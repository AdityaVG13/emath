use emath_world_ir::builtin::{WorldClass, builtin_worlds};

/// The exit-gate shape: at least five world classes, every identity
/// deterministic (recomputation stable) and pairwise distinct.
#[test]
fn at_least_five_classes_with_deterministic_distinct_identities() {
    let worlds = builtin_worlds();
    assert!(worlds.len() >= 5, "need at least five world classes");
    assert_eq!(worlds.len(), WorldClass::ALL.len());
    let mut seen = std::collections::BTreeMap::new();
    for world in &worlds {
        let first = world.identity();
        let second = world.identity();
        assert_eq!(first, second, "{} identity unstable", world.world.name);
        if let Some(previous) = seen.insert(first, world.class) {
            panic!(
                "identity collision between {previous:?} and {:?}",
                world.class
            );
        }
    }
    let classes: Vec<WorldClass> = worlds.iter().map(|world| world.class).collect();
    assert_eq!(classes, WorldClass::ALL.to_vec(), "stable class order");
}

/// Rebuilding the provider is deterministic end to end: same classes,
/// same canonical forms, same identities.
#[test]
fn builtin_provider_is_deterministic_across_rebuilds() {
    let first = builtin_worlds();
    let second = builtin_worlds();
    assert_eq!(first, second);
    for (a, b) in first.iter().zip(&second) {
        assert_eq!(a.world.canonical(), b.world.canonical());
        assert_eq!(a.identity(), b.identity());
    }
}

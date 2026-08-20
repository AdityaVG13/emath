//! SG-16 production path: verify a homomorphism, reject a seeded wrong
//! map, dedupe an isomorphic pair, quotient a mergeable table, and mine
//! shared invariants across a three-world portfolio.

use emath_genesis::{
    dedupe, mine_invariants, morphism_id, quotient, verify, MorphismError, OpTable, WorldMorphism,
    MORPHISM_VERSION,
};
use emath_world_ir::fnv1a64;

pub fn demo() -> u8 {
    println!("== demo world-morphisms ==");
    match run_demo() {
        Ok(()) => {
            println!("world-morphisms demo: ok");
            0
        }
        Err(error) => {
            eprintln!("world-morphisms demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    emath_genesis::morphism::check_version(MORPHISM_VERSION)
        .map_err(|error| format!("morphism version handshake refused: {error:?}"))?;

    let xor = OpTable {
        carrier_size: 2,
        cells: vec![0, 1, 1, 0],
    };
    let xnor = OpTable {
        carrier_size: 2,
        cells: vec![1, 0, 0, 1],
    };
    let and_table = OpTable {
        carrier_size: 2,
        cells: vec![0, 0, 0, 1],
    };
    let constant = OpTable {
        carrier_size: 2,
        cells: vec![0, 0, 0, 0],
    };

    let identity = WorldMorphism::identity(2)
        .map_err(|error| format!("identity morphism refused: {error:?}"))?;
    verify(&identity, &xor, &xor).map_err(|error| format!("identity verify refused: {error:?}"))?;
    println!(
        "world-morphisms|verify|identity|map=0,1|id={:016x}",
        morphism_id(&identity)
    );

    let wrong = WorldMorphism {
        source_size: 2,
        target_size: 2,
        map: vec![1, 0],
    };
    match verify(&wrong, &xor, &xor) {
        Err(MorphismError::NotAHomomorphism { pair: [0, 0] }) => {
            println!("world-morphisms|negative|pair=0,0|detected");
        }
        other => {
            return Err(format!(
                "seeded wrong map must fail at (0,0), got {other:?}"
            ))
        }
    }

    let grouped = dedupe(&[xor.clone(), xnor.clone(), constant.clone()])
        .map_err(|error| format!("dedupe refused: {error:?}"))?;
    if grouped.groups.len() != 2 {
        return Err(format!(
            "expected 2 iso groups, got {}",
            grouped.groups.len()
        ));
    }
    let xor_group = grouped
        .groups
        .iter()
        .find(|group| group.representative.cells == xor.cells)
        .ok_or_else(|| "dedupe missing XOR representative".to_string())?;
    if xor_group.dropped.len() != 1 || xor_group.dropped[0].witness.map != vec![1, 0] {
        return Err(format!(
            "expected XNOR dropped with witness 1,0, got {:?}",
            xor_group.dropped
        ));
    }
    let witness = xor_group.dropped[0]
        .witness
        .map
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "world-morphisms|dedupe|groups=2|dropped=1|witness={witness}|id={:016x}",
        grouped.receipt_id
    );

    let collapsed = quotient(&constant).map_err(|error| format!("quotient refused: {error:?}"))?;
    if collapsed.table.carrier_size != 1 || collapsed.classes != vec![vec![0, 1]] {
        return Err(format!(
            "constant table must quotient to one class, got {:?}",
            collapsed.classes
        ));
    }
    println!(
        "world-morphisms|quotient|const2|classes=1|table=0|id={:016x}",
        collapsed.morphism_id
    );

    let report = mine_invariants(&[xor, and_table, constant])
        .map_err(|error| format!("invariants refused: {error:?}"))?;
    if report.world_count != 3 {
        return Err(format!("expected 3 worlds, got {}", report.world_count));
    }
    if !report.shared.iter().any(|law| law == "commutative") {
        return Err(format!(
            "commutative must be shared, got {:?}",
            report.shared
        ));
    }
    if report.shared.iter().any(|law| law == "identity") {
        return Err("existential identity must not be shared with the constant table".to_string());
    }
    let identity_holds = report
        .laws
        .iter()
        .find(|verdict| verdict.law == "identity")
        .map(|verdict| verdict.holds.as_slice())
        .ok_or_else(|| "missing identity verdicts".to_string())?;
    if identity_holds != [true, true, false] {
        return Err(format!(
            "expected identity verdicts [true, true, false], got {identity_holds:?}"
        ));
    }
    let shared = report.shared.join(",");
    println!("world-morphisms|invariants|shared={shared}|identity=not-shared|worlds=3");

    let rows = [
        format!("verify|identity|map=0,1|id={:016x}", morphism_id(&identity)),
        "negative|pair=0,0|detected".to_string(),
        format!(
            "dedupe|groups=2|dropped=1|witness={witness}|id={:016x}",
            grouped.receipt_id
        ),
        format!(
            "quotient|const2|classes=1|table=0|id={:016x}",
            collapsed.morphism_id
        ),
        format!("invariants|shared={shared}|identity=not-shared|worlds=3"),
    ];
    let body = rows.join("\n");
    let receipt_id = fnv1a64(body.as_bytes());
    println!(
        "world-morphisms: rows={} receipt={receipt_id:016x}",
        rows.len()
    );
    Ok(())
}

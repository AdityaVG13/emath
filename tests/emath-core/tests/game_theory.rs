//! `emath-r3-game-theory-t9m8`: B41 finite-carrier game claims —
//! contract tests.
//!
//! Design resolution (per the bead): the C10 blocker is CLOSED, but
//! the admitted `.emath` generic surface remains the admission-table
//! follow-up; the nucleus carries `BimatrixGame` as a runtime-shaped
//! finite carrier (row/column payoff matrices). Nash equilibrium is a
//! CHECKABLE CLAIM (an assertion about a profile), never a search
//! promise: the checker verifies a claimed profile and returns a
//! typed verdict — it does NOT enumerate equilibria. Infinite/continuous
//! games and general Nash oracles REFUSE by name (honest fence per the
//! lane rules).
//!
//! Best-response computation is exact over the FINITE carrier:
//! ties are reported as a SET (a best-response set, not a silent
//! argmax pick). Mixed strategies are validated finite distributions
//! (mass 1 within 1e-9, never renormalized — the probability-cell
//! discipline); expected utility is the exact bilinear form. Support
//! enumeration for mixed Nash is the declared follow-up (it is a
//! computation, distinct from the claim checker).
//!
//! Failure-first: RED (E0432) until `game_theory` lands.

use emath_core::game_theory::{
    best_responses, expected_utility, BimatrixGame, MixedStrategy, PayoffMatrix,
};

const TOL: f64 = 1e-12;

/// Prisoner's dilemma: T=5, R=3, P=1, S=0 (row player's view; column
/// symmetric). Cooperate=0, Defect=1. Strict dominance of Defect.
fn prisoners_dilemma() -> BimatrixGame {
    BimatrixGame {
        row_payoffs: PayoffMatrix {
            rows: 2,
            columns: 2,
            entries: vec![3.0, 0.0, 5.0, 1.0],
        },
        column_payoffs: PayoffMatrix {
            rows: 2,
            columns: 2,
            entries: vec![3.0, 5.0, 0.0, 1.0],
        },
    }
}

#[test]
fn bimatrix_game_construction_validates_shape() {
    let game = prisoners_dilemma();
    assert_eq!(game.rows(), 2);
    assert_eq!(game.columns(), 2);
    // Ragged matrices refuse: a game with mismatched payoff counts is
    // not a game.
    let bad = BimatrixGame {
        row_payoffs: PayoffMatrix {
            rows: 2,
            columns: 2,
            entries: vec![1.0, 2.0, 3.0],
        },
        column_payoffs: PayoffMatrix {
            rows: 2,
            columns: 2,
            entries: vec![1.0, 2.0, 3.0, 4.0],
        },
    };
    assert!(bad.validate().is_err(), "ragged row matrix refuses");
}

#[test]
fn best_response_is_a_set_ties_included() {
    let game = prisoners_dilemma();
    // Against column C (0): row gets 3 (C) or 5 (D) → {D}.
    assert_eq!(best_responses(&game.row_payoffs, 0), vec![1]);
    // Against column D (1): row gets 0 (C) or 1 (D) → {D}.
    assert_eq!(best_responses(&game.row_payoffs, 1), vec![1]);
    // A tied game: matching pennies structure on a diagonal — both
    // columns give row the same payoff → BOTH are best responses.
    let tied = PayoffMatrix {
        rows: 2,
        columns: 2,
        entries: vec![2.0, 2.0, 2.0, 2.0],
    };
    assert_eq!(best_responses(&tied, 0), vec![0, 1], "ties are a set");
    assert_eq!(best_responses(&tied, 1), vec![0, 1]);
}

#[test]
fn nash_claim_checks_profiles_not_searches() {
    let game = prisoners_dilemma();
    // (D, D) IS a Nash equilibrium: neither unilateral deviation gains.
    assert!(game.is_nash_equilibrium(1, 1).unwrap());
    // (C, C) is NOT: row gains by defecting (3 → 5).
    assert!(!game.is_nash_equilibrium(0, 0).unwrap());
    // (C, D) is not (row gains 0 → 1); (D, C) is not (column gains).
    assert!(!game.is_nash_equilibrium(0, 1).unwrap());
    assert!(!game.is_nash_equilibrium(1, 0).unwrap());
    // Out-of-range profiles refuse (not "false" — the claim is about a
    // real profile).
    assert!(game.is_nash_equilibrium(2, 0).is_err());
    assert!(game.is_nash_equilibrium(0, 5).is_err());
}

#[test]
fn coordination_game_has_two_pure_nash() {
    // Battle of the sexes flavor: both (0,0) and (1,1) are Nash.
    let game = BimatrixGame {
        row_payoffs: PayoffMatrix {
            rows: 2,
            columns: 2,
            entries: vec![2.0, 0.0, 0.0, 1.0],
        },
        column_payoffs: PayoffMatrix {
            rows: 2,
            columns: 2,
            entries: vec![1.0, 0.0, 0.0, 2.0],
        },
    };
    assert!(game.is_nash_equilibrium(0, 0).unwrap());
    assert!(game.is_nash_equilibrium(1, 1).unwrap());
    assert!(!game.is_nash_equilibrium(0, 1).unwrap());
    assert!(!game.is_nash_equilibrium(1, 0).unwrap());
}

#[test]
fn mixed_strategy_validates_mass_and_shape() {
    assert!(MixedStrategy::new(vec![0.5, 0.5]).is_ok());
    assert!(MixedStrategy::new(vec![1.0, 0.0]).is_ok(), "pure as degenerate mix");
    assert!(
        MixedStrategy::new(vec![0.6, 0.5]).is_err(),
        "mass 1.1 refuses (never renormalized)"
    );
    assert!(MixedStrategy::new(vec![]).is_err(), "empty carrier refuses");
    assert!(
        MixedStrategy::new(vec![-0.1, 1.1]).is_err(),
        "negative weight refuses"
    );
}

#[test]
fn expected_utility_is_the_exact_bilinear_form() {
    let game = prisoners_dilemma();
    // Row mixes (0.25 C, 0.75 D) against pure D: row utility =
    // 0.25·0 + 0.75·1 = 0.75.
    let row_mix = MixedStrategy::new(vec![0.25, 0.75]).unwrap();
    let defect = MixedStrategy::new(vec![0.0, 1.0]).unwrap();
    let utility = expected_utility(&game.row_payoffs, &row_mix, &defect).unwrap();
    assert!((utility - 0.75).abs() < TOL);
    // 50/50 against 50/50: 0.25·(3+0+5+1) = 2.25.
    let half = MixedStrategy::new(vec![0.5, 0.5]).unwrap();
    let utility = expected_utility(&game.row_payoffs, &half, &half).unwrap();
    assert!((utility - 2.25).abs() < TOL);
}

#[test]
fn mixed_profile_against_pure_supports_nash_claim_check() {
    // The D-D profile as degenerate mixes is still Nash under the
    // mixed checker (a pure profile is a degenerate mixed profile).
    let game = prisoners_dilemma();
    let d = MixedStrategy::new(vec![0.0, 1.0]).unwrap();
    let c = MixedStrategy::new(vec![1.0, 0.0]).unwrap();
    assert!(game.is_mixed_nash(&d, &d).unwrap());
    assert!(!game.is_mixed_nash(&c, &c).unwrap());
}

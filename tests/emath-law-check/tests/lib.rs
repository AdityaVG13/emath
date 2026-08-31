mod law_check {
    use emath_lab_core::calibration::FittedTable;
    use emath_law_check::{CheckerError, FiniteLawChecker, Law, WorldObligation};
    use emath_term::SymbolId;
    use emath_world_ir::{WorldId, fnv1a64};

    fn commutative_obligation(operator: &str) -> WorldObligation {
        WorldObligation {
            id: fnv1a64(format!("test:{operator}").as_bytes()),
            law: Law::Commutative(SymbolId(operator.to_string())),
        }
    }

    #[test]
    fn empty_table_is_not_a_vacuous_pass() {
        // Zero rows must be a typed refusal (EmptyTable), never
        // `passed: true` from an empty carrier.
        let table = FittedTable::from_cells(
            SymbolId("op".to_string()),
            2,
            std::collections::BTreeMap::new(),
        );
        let error = FiniteLawChecker
            .check(WorldId(0), &table, &[commutative_obligation("op")])
            .expect_err("an empty table cannot pass a law");
        assert_eq!(error, CheckerError::EmptyTable);
    }

    #[test]
    fn untotal_table_is_refused_not_passed() {
        // A binary table over a two-element carrier missing one of its
        // four rows must refuse (Untotal), not pass the law over the
        // rows that happen to exist.
        let mut cells = std::collections::BTreeMap::new();
        for (left, right, value) in [("a", "a", "a"), ("a", "b", "a"), ("b", "a", "a")] {
            cells.insert(vec![left.to_string(), right.to_string()], value.to_string());
        }
        let table = FittedTable::from_cells(SymbolId("op".to_string()), 2, cells);
        let error = FiniteLawChecker
            .check(WorldId(0), &table, &[commutative_obligation("op")])
            .expect_err("an untotal table cannot pass a law");
        assert!(matches!(error, CheckerError::Untotal { .. }));
    }
}

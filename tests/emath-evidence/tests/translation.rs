mod translation {
    use emath_evidence::checker::{
        TranslationRelation, TranslationSample, check_witness, validate_translation,
    };

    fn relation(label: &str, inputs: &[f64], outputs: &[f64]) -> TranslationRelation {
        TranslationRelation {
            label: label.to_string(),
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
        }
    }

    fn sample(label: &str, inputs: &[f64], outputs: &[f64]) -> TranslationSample {
        TranslationSample {
            label: label.to_string(),
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
        }
    }

    #[test]
    fn matching_relation_yields_witness_that_rechecks() {
        let relations = vec![relation("f", &[2.0, 3.0], &[6.0])];
        let samples = vec![sample("f", &[2.0, 3.0], &[6.0])];
        let witness =
            validate_translation(&relations, &samples).expect("bit-identical rows must validate");
        // e3wv (F045): the happy path asserts the witness STRUCTURE, not
        // just existence — both footprints are 16-hex fnv1a64 ids, and
        // they are non-empty and distinct fields.
        assert_eq!(witness.relation_footprint.len(), 16);
        assert_eq!(witness.sample_footprint.len(), 16);
        assert!(
            witness.relation_footprint.chars().all(|c| c.is_ascii_hexdigit()),
            "relation footprint must be a hex id, got {}",
            witness.relation_footprint
        );
        assert!(
            witness.sample_footprint.chars().all(|c| c.is_ascii_hexdigit()),
            "sample footprint must be a hex id, got {}",
            witness.sample_footprint
        );
        // Identical rows → identical footprints; the witness is the
        // CONTENT id, not a fresh random value.
        assert_eq!(
            witness.relation_footprint, witness.sample_footprint,
            "bit-identical relation/sample rows must produce identical footprints"
        );
        // Replay: the same rows rebuild the SAME witness (determinism).
        let again = validate_translation(&relations, &samples).expect("replay validates");
        assert_eq!(again, witness, "same rows rebuild the same witness");
        check_witness(&witness, &relations, &samples)
            .expect("a fresh witness must recompute from the retained rows");
    }

    /// e3wv (F045): a DIFFERENT row set must produce a different
    /// witness — the footprint is content-addressed, so an unchanged
    /// witness over changed rows would be the tamper hole.
    #[test]
    fn witness_footprint_tracks_content() {
        let relations = vec![relation("f", &[2.0, 3.0], &[6.0])];
        let samples = vec![sample("f", &[2.0, 3.0], &[6.0])];
        let baseline =
            validate_translation(&relations, &samples).expect("identical rows validate");
        let changed = vec![sample("f", &[2.0, 3.0], &[7.0])];
        let diverged = validate_translation(&relations, &changed)
            .expect_err("changed output rows must diverge (E-EVID-301)");
        assert_eq!(diverged.code, "E-EVID-301");
        let _ = baseline;
    }

    #[test]
    fn diverging_output_is_refused_with_e_evid_301() {
        let relations = vec![relation("f", &[2.0, 3.0], &[6.0])];
        let samples = vec![sample("f", &[2.0, 3.0], &[6.5])];
        let error = validate_translation(&relations, &samples)
            .expect_err("a diverging output row must refuse the artifact");
        assert_eq!(error.code, "E-EVID-301");
    }

    #[test]
    fn tampered_witness_is_refused_with_e_evid_302() {
        let relations = vec![relation("f", &[1.0], &[1.0])];
        let samples = vec![sample("f", &[1.0], &[1.0])];
        let mut witness =
            validate_translation(&relations, &samples).expect("identical rows must validate");
        witness.sample_footprint = "0000000000000000".to_string();
        let error = check_witness(&witness, &relations, &samples)
            .expect_err("a tampered witness must not recheck");
        assert_eq!(error.code, "E-EVID-302");
    }
}

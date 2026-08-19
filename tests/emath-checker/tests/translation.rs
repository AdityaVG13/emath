mod translation {
    use emath_checker::{
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
        check_witness(&witness, &relations, &samples)
            .expect("a fresh witness must recompute from the retained rows");
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

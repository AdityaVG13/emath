//! Tests for graph.rs, migrated out of production code.
//! All items under test are public crate surface.

use emath_cli::layout::{LayoutError, LAYOUT_VERSION, check_version, parse_latex};

    #[test]
    fn graph_unknown_version_refused() {
        assert_eq!(check_version(LAYOUT_VERSION), Ok(()));
        assert_eq!(
            check_version(LAYOUT_VERSION + 1),
            Err(LayoutError::UnknownVersion {
                version: LAYOUT_VERSION + 1
            })
        );
    }

    #[test]
    fn graph_canonical_identical_across_rebuilds() {
        let source = r"\sum_{i=1}^{3} i";
        let first = parse_latex(source).expect("parse");
        let second = parse_latex(source).expect("parse");
        assert_eq!(first.canonical(), second.canonical());
        assert_eq!(first.graph_id(), second.graph_id());
        assert_eq!(first.source().as_bytes(), source.as_bytes());
    }

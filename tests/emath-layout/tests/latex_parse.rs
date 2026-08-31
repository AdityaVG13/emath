//! Tests for latex.rs, migrated out of production code.
//! All items under test are public crate surface.

use emath_cli::layout::{parse_latex, to_binder_term};
use emath_cli::layout::LayoutError;
use emath_genesis::{BinderBudget, BinderDomain, BinderFamily, BinderKind, BinderTerm};
use emath_term::SymbolId;

    #[test]
    fn latex_source_preserved_byte_exact() {
        let source = r"\sum_{i=1}^{3} i";
        let graph = parse_latex(source).expect("parse");
        assert_eq!(graph.source(), source);
        assert_eq!(graph.source().as_bytes(), source.as_bytes());
    }

    #[test]
    fn latex_sum_lowers_to_structural_finite_range_and_expands() {
        let graph = parse_latex(r"\sum_{i=1}^{3} i").expect("parse");
        let term = to_binder_term(&graph).expect("lower");
        let BinderTerm::Bind(binder) = term else {
            panic!("expected a sum binder, got {term:?}");
        };
        assert_eq!(binder.kind, BinderKind::Sum);
        assert_eq!(binder.family, BinderFamily::Structural);
        assert_eq!(
            binder.domain,
            BinderDomain::FiniteRange {
                lower: 1,
                upper: 3
            }
        );
        let expanded = binder
            .expand(&SymbolId("+".to_string()), BinderBudget::default())
            .expect("expand");
        assert_eq!(
            expanded.canonical(),
            "apply(+,apply(+,const(1),const(2)),const(3))"
        );
    }

    #[test]
    fn latex_unknown_macro_refused_with_offset() {
        let error = parse_latex(r"x+\foo").expect_err("unknown macro");
        assert_eq!(
            error,
            LayoutError::UnknownMacro {
                name: "foo".to_string(),
                offset: 2,
            }
        );
    }

    #[test]
    fn latex_unterminated_dollar_refused() {
        let error = parse_latex("hello $foo").expect_err("unterminated");
        assert_eq!(error, LayoutError::UnterminatedDollar { offset: 6 });
    }

    #[test]
    fn latex_formula_region_spans_byte_exact() {
        let source = r"see $\sum_{i=1}^{3} i$ please";
        let graph = parse_latex(source).expect("parse");
        let region = graph
            .formula_regions()
            .next()
            .expect("one formula region");
        let (start, end) = region.source_span;
        assert_eq!(&source[start..end], r"$\sum_{i=1}^{3} i$");
    }

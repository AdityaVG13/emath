//! Refinement types seed (bead emath-r3-refinement-types-rrhd, 05 §7.1).
//!
//! Contracts:
//! - **Inline where-refinement rows refuse naming the seed design**:
//!   `p: Float64 where 0 <= self and self <= 1` refuses `E-SYN-101`
//!   naming the total/decidable fragment, identity recording, named
//!   conflict diagnostics (ch16 gate 6), and the no-launder cast
//!   policy (Certified → nothing, receipt-visible) — previously the
//!   row half-parsed and the dangling `where ...` predicate died with
//!   a generic `only 'name: Type' declarations are allowed` error that
//!   named nothing;
//! - **the admitted refinement surface is unchanged**: the domain
//!   annotation `Type in [lo, hi]` (U5) still admits, and the
//!   `type X = T` alias keeps its own `E-TYPE-111` refusal;
//! - ordinary declarations admit unchanged.
//!
//! Design prose of record: ch.5 "Refinements-everywhere: the seed
//! contract (05 section 7.1)".

use emath_core::limits::Limits;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const WHERE_REFINEMENT_ROW: &str = "\
emath function RefProbe:
    inputs:
        p: Float64 where 0 <= self and self <= 1

    definitions:
        f = p * 2
";

const DOMAIN_ANNOTATION: &str = "\
emath function RefProbe3:
    inputs:
        p: Float64 in [0.0, 1.0]

    definitions:
        f = p * 2
";

const TYPE_ALIAS: &str = "\
emath function RefProbe:
    type Probability = Float64 where 0 <= self and self <= 1

    inputs:
        p: Float64 = 0.5

    definitions:
        f = p * 2
";

const PLAIN_MODEL: &str = "\
emath function PlainRefProbe:
    inputs:
        p: Float64 = 0.5

    definitions:
        f = p * 2
";

#[test]
fn inline_where_refinement_refuses_naming_seed() {
    let errors = check(WHERE_REFINEMENT_ROW, "where-fence");
    assert!(
        errors.iter().any(|e| e.contains("where <predicate>")
            && e.contains("decidable")),
        "the inline where-refinement row must refuse naming the \
         total/decidable fragment contract; got: {errors:#?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("Certified")),
        "the where fence must name the no-launder cast policy (Certified \
         downgrades to nothing); got: {errors:#?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("only `name: Type` declarations")),
        "the refinement row must never die with the generic row-shape \
         error; got: {errors:#?}"
    );
}

#[test]
fn domain_annotation_and_alias_surfaces_unchanged() {
    let errors = check(DOMAIN_ANNOTATION, "domain-guard");
    assert!(
        errors.is_empty(),
        "the domain annotation (U5) is the ADMITTED refinement surface \
         and must admit unchanged; got: {errors:#?}"
    );
    let alias_errors = check(TYPE_ALIAS, "alias-guard");
    assert!(
        alias_errors.iter().any(|e| e.starts_with("E-TYPE-111")),
        "type aliases keep their own E-TYPE-111 refusal (not this seed's \
         fence); got: {alias_errors:#?}"
    );
}

#[test]
fn plain_models_admit_unchanged() {
    let errors = check(PLAIN_MODEL, "refine-plain-guard");
    assert!(
        errors.is_empty(),
        "the refinement seed must not affect ordinary models; got: \
         {errors:#?}"
    );
}

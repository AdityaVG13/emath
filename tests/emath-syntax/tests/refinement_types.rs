//! Refinement types seed (05 §7.1).
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

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
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

use emath_test_harness::{Probe, Source, boot};

#[test]
fn refinement_types() {
    boot();
    let mut probe = Probe::new("Refinement types seed (05 §7.1). Contracts: - **Inline where-refinement rows refuse naming the seed design**: `p: Float64 where 0 <= self and self <=");
    probe.case("inline_where_refinement_refuses_naming_seed", |p| {
    let f0 = p.failures().len();

    let errors = check(WHERE_REFINEMENT_ROW, "where-fence");
    p.demand("1",errors
            .iter()
            .any(|e| e.contains("where <predicate>") && e.contains("decidable")), format!(
        "the inline where-refinement row must refuse naming the \
         total/decidable fragment contract; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",errors.iter().any(|e| e.contains("Certified")), format!(
        "the where fence must name the no-launder cast policy (Certified \
         downgrades to nothing); got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",!errors
            .iter()
            .any(|e| e.contains("only `name: Type` declarations")), format!(
        "the refinement row must never die with the generic row-shape \
         error; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("domain_annotation_and_alias_surfaces_unchanged", |p| {
    let f0 = p.failures().len();

    let errors = check(DOMAIN_ANNOTATION, "domain-guard");
    p.demand("1",errors.is_empty(), format!(
        "the domain annotation (U5) is the ADMITTED refinement surface \
         and must admit unchanged; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }
    let alias_errors = check(TYPE_ALIAS, "alias-guard");
    p.demand("2",alias_errors.iter().any(|e| e.starts_with("E-TYPE-111")), format!(
        "type aliases keep their own E-TYPE-111 refusal (not this seed's \
         fence); got: {alias_errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("plain_models_admit_unchanged", |p| {
    let f0 = p.failures().len();

    let errors = check(PLAIN_MODEL, "refine-plain-guard");
    p.demand("1",errors.is_empty(), format!(
        "the refinement seed must not affect ordinary models; got: \
         {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}

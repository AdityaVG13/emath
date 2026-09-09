//! `emath-store` evidence-state schema tests (migrated from
//! `crates/emath-store/src/lib.rs`).

use emath_core::{
    ArtifactId, EvidenceId, MeaningId, PackId, RecipeId, SnapshotId, SourceId, ViewId,
};
use emath_ir::{ExprId, MeaningError, NumericProfile, SemanticPackage};
use emath_sema::CompilerSession;
use emath_store::schema::{
    self, CLAIM_STATUS_FAIL, CLAIM_STATUS_OK, CLAIM_STATUS_PENDING, SCHEMA_SQL,
};
use emath_test_harness::{Case, Probe, boot, check_all};

fn admit(source: &str) -> Result<SemanticPackage, String> {
    boot();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned("meaning-id.emath", source);
    let errors = result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(result.package)
    } else {
        Err(format!("{errors:#?}"))
    }
}

#[test]
fn probe() {
    let mut p = Probe::new(
        "store schema is closed DDL; identities isolate domains; meaning follows admitted math not prose",
    );
    p.case("ddl", |p| {
        p.contains("artifacts", SCHEMA_SQL, "CREATE TABLE IF NOT EXISTS artifacts (");
        p.contains("evidence", SCHEMA_SQL, "CREATE TABLE IF NOT EXISTS evidence (");
        p.contains("status", SCHEMA_SQL, "status IN ('ok', 'fail', 'pending')");
        p.contains(
            "pk",
            SCHEMA_SQL,
            "PRIMARY KEY (artifact_id, claim, seq)",
        );
    });
    p.case("claim-status", |p| {
        if let Err(message) = check_all(
            &[
                Case::new("ok", CLAIM_STATUS_OK, true),
                Case::new("fail", CLAIM_STATUS_FAIL, true),
                Case::new("pending", CLAIM_STATUS_PENDING, true),
                Case::new("empty", "", false),
                Case::new("okay", "okay", false),
                Case::new("OK", "OK", false),
                Case::new("padded", " pending", false),
            ],
            |status| schema::valid_claim_status(status),
        ) {
            p.fail("bounds", message);
        }
    });
    p.case("identity-domains", |p| {
        let payload = b"same canonical payload";
        let source = SourceId::from_bytes(payload);
        let meaning = MeaningId::from_bytes(payload);
        let evidence = EvidenceId::from_bytes(payload);
        let view = ViewId::from_bytes(payload);
        let recipe = RecipeId::from_bytes(payload);
        let artifact = ArtifactId::from_bytes(payload);
        let snapshot = SnapshotId::from_bytes(payload);
        let pack = PackId::from_bytes(payload);
        p.demand("source-prefix", source.as_str().starts_with(SourceId::PREFIX), "source prefix");
        p.demand("meaning-prefix", meaning.as_str().starts_with(MeaningId::PREFIX), "meaning prefix");
        p.demand(
            "evidence-prefix",
            evidence.as_str().starts_with(EvidenceId::PREFIX),
            "evidence prefix",
        );
        p.demand("view-prefix", view.as_str().starts_with(ViewId::PREFIX), "view prefix");
        p.demand("recipe-prefix", recipe.as_str().starts_with(RecipeId::PREFIX), "recipe prefix");
        p.demand(
            "artifact-prefix",
            artifact.as_str().starts_with(ArtifactId::PREFIX),
            "artifact prefix",
        );
        p.demand(
            "snapshot-prefix",
            snapshot.as_str().starts_with(SnapshotId::PREFIX),
            "snapshot prefix",
        );
        p.demand("pack-prefix", pack.as_str().starts_with(PackId::PREFIX), "pack prefix");
        let identities = [
            source.as_str(),
            meaning.as_str(),
            evidence.as_str(),
            view.as_str(),
            recipe.as_str(),
            artifact.as_str(),
            snapshot.as_str(),
            pack.as_str(),
        ];
        for (index, identity) in identities.iter().enumerate() {
            p.eq(
                format!("{index}/len"),
                identity.len(),
                identities[index].rfind(':').unwrap() + 65,
            );
            p.demand(
                format!("{index}/unique"),
                identities[index + 1..].contains(identity) == false,
                format!("{identity} collided with a later domain"),
            );
        }
    });
    p.case("source-mutation", |p| {
        p.eq(
            "empty",
            SourceId::from_bytes(b"").as_str().to_string(),
            "emath:source:v1:acd0aeb36f91ce4893b33dae198e072135f61d5dbcb5e88899dd01ffc1cb0716"
                .to_string(),
        );
        let original = SourceId::from_bytes(b"emath function square:\n  return x * x\n");
        let mutated = SourceId::from_bytes(b"emath function square:\n  return x * x!");
        p.ne("bang", original.clone(), mutated);
        let encoded = original.to_string();
        match encoded.parse::<SourceId>() {
            Ok(parsed) => {
                p.eq("roundtrip", parsed, original);
            }
            Err(error) => {
                p.fail("roundtrip", format!("{error}"));
            }
        }
        p.demand(
            "cross-domain",
            encoded
                .replace("emath:source:", "emath:meaning:")
                .parse::<SourceId>()
                .is_err(),
            "source wire must not parse as meaning",
        );
        p.demand(
            "case",
            encoded.to_uppercase().parse::<SourceId>().is_err(),
            "uppercase source wire must refuse",
        );
        p.demand(
            "truncated",
            encoded[..encoded.len() - 1].parse::<SourceId>().is_err(),
            "truncated source wire must refuse",
        );
    });
    p.case("alpha-rename", |p| {
        let left = match admit(
            "emath function f:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n",
        ) {
            Ok(package) => package_meaning(p, "left", package),
            Err(error) => {
                p.fail("left", error);
                return;
            }
        };
        let right = match admit(
            "# same admitted mathematics\n\nemath function square:\n    inputs:\n        value: Float64\n    definitions:\n        result = value * value\n",
        ) {
            Ok(package) => package_meaning(p, "right", package),
            Err(error) => {
                p.fail("right", error);
                return;
            }
        };
        if let (Some(left), Some(right)) = (left, right) {
            p.eq("rename", left, right);
        }
        let sum_i = match admit(
            "emath function Sum:\n    outputs:\n        total: Float64\n    definitions:\n        total = sum i in 1..6: i\n",
        ) {
            Ok(package) => package_meaning(p, "sum-i", package),
            Err(error) => {
                p.fail("sum-i", error);
                return;
            }
        };
        let sum_k = match admit(
            "emath function Renamed:\n    outputs:\n        answer: Float64\n    definitions:\n        answer = sum k in 1..6: k\n",
        ) {
            Ok(package) => package_meaning(p, "sum-k", package),
            Err(error) => {
                p.fail("sum-k", error);
                return;
            }
        };
        if let (Some(sum_i), Some(sum_k)) = (sum_i, sum_k) {
            p.eq("binder", sum_i, sum_k);
        }
    });
    p.case("notation-alias", |p| {
        let glyph = match admit(
            r#"notation infixl 40 "⊕" => core::math::pow alias "pw"
emath function Power:
    inputs:
        x: Float64
        y: Float64
    definitions:
        result = x⊕y
"#,
        ) {
            Ok(package) => package_meaning(p, "glyph", package),
            Err(error) => {
                p.fail("glyph", error);
                return;
            }
        };
        let alias = match admit(
            r#"notation infixl 40 "⊕" => core::math::pow alias "pw"
emath function PowerAlias:
    inputs:
        a: Float64
        b: Float64
    definitions:
        value = a pw b
"#,
        ) {
            Ok(package) => package_meaning(p, "alias", package),
            Err(error) => {
                p.fail("alias", error);
                return;
            }
        };
        if let (Some(glyph), Some(alias)) = (glyph, alias) {
            p.eq("alias", glyph, alias);
        }
    });
    p.case("semantic-policy", |p| {
        let base = match admit(
            "emath function Square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n",
        ) {
            Ok(package) => package,
            Err(error) => {
                p.fail("base", error);
                return;
            }
        };
        let Some(base_id) = package_meaning(p, "base", base.clone()) else {
            return;
        };
        let changed = match admit(
            "emath function Square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x + 1.0\n",
        ) {
            Ok(package) => package_meaning(p, "changed", package),
            Err(error) => {
                p.fail("changed", error);
                return;
            }
        };
        if let Some(changed) = changed {
            p.ne("body", base_id.clone(), changed);
        }
        let mut prose = base.clone();
        prose.declarations[0].about = Some("non-authoritative documentation".to_string());
        if let Some(prose_id) = package_meaning(p, "prose", prose) {
            p.eq("about", base_id.clone(), prose_id);
        }
        let mut interval = base.clone();
        interval.declarations[0].compile_spec.numeric = NumericProfile::IntervalF64;
        if let Some(interval_id) = package_meaning(p, "interval", interval) {
            p.ne("numeric", base_id.clone(), interval_id);
        }
        let dependency = MeaningId::from_bytes(b"dependency meaning");
        match (base.meaning_id(&[]), base.meaning_id(std::slice::from_ref(&dependency))) {
            (Ok(plain), Ok(with_dep)) => {
                p.ne("dep", plain, with_dep);
            }
            (plain, with_dep) => {
                p.fail("dep", format!("meaning_id failed: {plain:?} / {with_dep:?}"));
            }
        }
        let other_dependency = MeaningId::from_bytes(b"other dependency meaning");
        match (
            base.meaning_id(&[dependency.clone(), other_dependency.clone(), dependency]),
            base.meaning_id(&[
                other_dependency,
                MeaningId::from_bytes(b"dependency meaning"),
            ]),
        ) {
            (Ok(left), Ok(right)) => {
                p.eq("dep-order", left, right);
            }
            (left, right) => {
                p.fail("dep-order", format!("{left:?} / {right:?}"));
            }
        }
        let mut malformed = base;
        *malformed.declarations[0]
            .definitions
            .values_mut()
            .next()
            .unwrap() = ExprId(u32::MAX);
        p.demand(
            "missing-expr",
            matches!(
                malformed.meaning_id(&[]),
                Err(MeaningError::MissingExpr(ExprId(u32::MAX)))
            ),
            "dangling ExprId must be MissingExpr",
        );
    });
    p.finish();
}

fn package_meaning(p: &mut Probe, name: &str, package: SemanticPackage) -> Option<MeaningId> {
    match package.meaning_id(&[]) {
        Ok(id) => Some(id),
        Err(error) => {
            p.fail(name, format!("meaning_id: {error}"));
            None
        }
    }
}

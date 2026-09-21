//! The shared tree substrate (bead emath-shared-tree-view-make-bp8nu,
//! definition table emath-quote-body-defs-trto7): the distilled
//! tree, the node-record family `quote.view` produces, the minted
//! Scope witnesses, the reconstruction `quote.make` consumes, the
//! scalar tree evaluator over the same kernels as the compiled
//! template lane, and the `quote.body` definition-table unfold.
//! These pins are the substrate's laws: mint format and determinism,
//! the forged-scope refusal, view layouts matching the VM's field
//! layouts, view -> make round trips (and a MODIFIED rebuild
//! changing the value), the evaluator's carrier rules, and the
//! Available/Opaque body records.

use std::rc::Rc;

use emath_rt::code::{
    code_mul, evaluate_expr, open_expr, substitute_expr, CodeValue,
};
use emath_rt::code_tree::{
    check_scope, dependency_snapshot_tree, evaluate_tree, free_names_tree, make_quoted, node_eq,
    node_field, node_index, node_record, node_tag, quote_body_node, rebuild_node, reset_mint,
    substitute_tree, verify_deps, view_node, view_quoted, view_tree, CodeTree, DefinitionRow,
    DefinitionTable, ModuleTable, NodeValue, TreeBinary, TreeUnary,
};
use emath_test_harness::{Probe, boot};

/// The fixture template `(x + c) * 4` as a distilled tree - the same
/// shape the tree_view fixture's `TreeTemplate` quotes.
fn template_tree() -> CodeTree {
    CodeTree::Binary {
        op: TreeBinary::Mul,
        left: Box::new(CodeTree::Binary {
            op: TreeBinary::Add,
            left: Box::new(CodeTree::Path(vec!["x".to_string()])),
            right: Box::new(CodeTree::Path(vec!["c".to_string()])),
        }),
        right: Box::new(CodeTree::Literal(CodeValue::Int(4))),
    }
}

/// The template as a dual Code value with a compiled factory (the
/// static-literal shape the backend emits).
fn template_code() -> emath_rt::code::ExprCode {
    open_expr(
        vec!["c".to_string(), "x".to_string()],
        template_tree(),
        Some(Rc::new(|values: &[CodeValue]| {
            code_mul(
                &emath_rt::code::code_add(&values[1], &values[0])?,
                &CodeValue::Int(4),
            )
        })),
        std::collections::BTreeMap::new(),
    )
}

/// Collect the Scope token strings from a view tree, in walk order.
fn scope_tokens(value: &NodeValue, tokens: &mut Vec<String>) {
    match value {
        NodeValue::Record(type_name, fields) => {
            if type_name == "Scope" {
                if let Some(NodeValue::Record(token, _)) = fields.get("token") {
                    tokens.push(token.clone());
                }
            }
            for field in fields.values() {
                scope_tokens(field, tokens);
            }
        }
        NodeValue::Sequence(items) | NodeValue::Tuple(items) => {
            for item in items {
                scope_tokens(item, tokens);
            }
        }
        _ => {}
    }
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("code tree: shared view/make substrate, minted scopes, scalar tree evaluation");

    // 1. Minting law: the token format is `#scope.{id}`, ids climb in
    //    walk order, and a fresh state mints the identical sequence
    //    for the same tree (deterministic per run). The 2-level walk
    //    mints one Scope per child fragment of the viewed node - the
    //    template's root Binary has two children.
    p.case("mint-format-and-determinism", |p| {
        reset_mint();
        let view = view_tree(&template_tree(), &ModuleTable::EMPTY);
        let mut first = Vec::new();
        scope_tokens(&view, &mut first);
        p.eq(
            "token-format",
            first.clone(),
            vec![
                "#scope.1".to_string(),
                "#scope.2".to_string(),
            ],
        );
        reset_mint();
        let mut second = Vec::new();
        scope_tokens(&view_tree(&template_tree(), &ModuleTable::EMPTY), &mut second);
        p.eq("per-run-determinism", second, first);
    });

    // 2. Forged-scope law: a Fragment package whose Scope witness
    //    carries an id this run did not mint refuses by the VM's
    //    name; a minted witness passes.
    p.case("forged-scope-refusal", |p| {
        reset_mint();
        let view = view_tree(&template_tree(), &ModuleTable::EMPTY);
        // The forged-scope check applies to Fragment packages (the
        // view's children), not the top-level node record.
        let children = match node_field(&view, "children") {
            Ok(NodeValue::Sequence(items)) => items,
            _ => Vec::new(),
        };
        let fragment = children
            .first()
            .cloned()
            .expect("the view carries fragment children");
        reset_mint();
        match check_scope(&fragment) {
            Ok(()) => {
                p.fail("stale-view-refuses", "an unminted witness set must refuse");
            }
            Err(fault) => {
                p.demand(
                    "vm-refusal-name",
                    fault == "invalid_code_construction: forged Scope witness",
                    &format!("refusal must match the VM's: {fault}"),
                );
            }
        }
        // A freshly minted witness admits.
        reset_mint();
        let fresh = view_tree(&template_tree(), &ModuleTable::EMPTY);
        let fresh_children = match node_field(&fresh, "children") {
            Ok(NodeValue::Sequence(items)) => items,
            _ => Vec::new(),
        };
        p.demand(
            "minted-witness-admits",
            fresh_children
                .first()
                .map(|child| check_scope(child).is_ok())
                .unwrap_or(false),
            "a witness minted this run must admit",
        );
    });

    // 3. View layouts: a Binary views as a Call node with the scalar
    //    op TAG as callee, Code subterms in `args`, minted Fragment
    //    packages in `children`; a Local names itself; a Literal
    //    carries its value; subterm Code values view recursively.
    p.case("view-layouts", |p| {
        reset_mint();
        let view = view_tree(&template_tree(), &ModuleTable::EMPTY);
        let NodeValue::Record(kind, fields) = &view else {
            p.fail("root-is-record", "the root view must be a node record");
            return;
        };
        p.eq("root-kind-call", kind.clone(), "Call".to_string());
        p.eq(
            "kind-field-tag",
            node_eq(fields.get("kind").expect("kind field"), &node_tag("Call")),
            Ok(true),
        );
        p.eq(
            "callee-is-add-tag",
            node_eq(fields.get("callee").expect("callee"), &node_tag("Mul")),
            Ok(true),
        );
        let args = match fields.get("args") {
            Some(NodeValue::Sequence(items)) => items.len(),
            _ => 0,
        };
        p.eq("args-are-code-children", args, 2);
        // The left argument is a Code value whose own tree views as
        // the inner Add call.
        match fields.get("args") {
            Some(NodeValue::Sequence(items)) => match &items[0] {
                NodeValue::Code(code) => {
                    let inner = view_quoted(code.as_ref(), &ModuleTable::EMPTY);
                    p.eq(
                        "subterm-views-recursively",
                        node_eq(
                            &node_field(&inner, "kind").expect("inner kind"),
                            &node_tag("Call"),
                        ),
                        Ok(true),
                    );
                    p.eq(
                        "subterm-callee-tag",
                        node_eq(
                            &node_field(&inner, "callee").expect("inner callee"),
                            &node_tag("Add"),
                        ),
                        Ok(true),
                    );
                }
                other => {
                    p.fail(
                        "args-hold-code",
                        &format!("an arg must be a Code value, found {other:?}"),
                    );
                }
            },
            _ => {
                p.fail("args-hold-code", "the args field must be a sequence");
            }
        }
        // A path views as a Local naming itself.
        let local = view_tree(&CodeTree::Path(vec!["x".to_string()]), &ModuleTable::EMPTY);
        p.eq(
            "local-name",
            node_eq(
                &node_field(&local, "name").expect("local name"),
                &node_tag("x"),
            ),
            Ok(true),
        );
        // A literal carries its value.
        let literal = view_tree(&CodeTree::Literal(CodeValue::Int(7)), &ModuleTable::EMPTY);
        p.eq(
            "literal-value",
            node_eq(
                &node_field(&literal, "value").expect("literal value"),
                &NodeValue::Scalar(CodeValue::Int(7)),
            ),
            Ok(true),
        );
    });

    // 4. Round trips: view -> make reproduces the tree exactly; a
    //    MODIFIED rebuild (the inner right operand becomes a made
    //    literal 5) changes the tree and the computed value. Tree
    //    comparison goes through the node family's Code equality
    //    (tree + dependencies), the same law the walk's `==` uses.
    p.case("view-make-round-trip", |p| {
        reset_mint();
        let code = template_code();
        let view = view_quoted(&code, &ModuleTable::EMPTY);
        let remade = make_quoted(&view, &ModuleTable::EMPTY).expect("round trip rebuilds");
        p.eq(
            "identity-round-trip",
            node_eq(
                &NodeValue::Code(Box::new(remade.clone())),
                &NodeValue::Code(Box::new(template_code())),
            ),
            Ok(true),
        );
        // The made code has no factory: evaluation runs the tree
        // evaluator, and computes the same value as the factory lane.
        let closed = substitute_expr(
            &substitute_expr(&remade, "c", CodeValue::Int(1)),
            "x",
            CodeValue::Int(2),
        );
        p.eq(
            "made-tree-evaluates",
            evaluate_expr(&closed, &ModuleTable::EMPTY),
            Ok(CodeValue::Int(12)),
        );
        // The modified rebuild: bump the inner constant, then rebuild
        // the OUTER call with the remade inner code as its left
        // argument - the walk consumer's edit path.
        let node = view_quoted(&code, &ModuleTable::EMPTY);
        let args = match node_field(&node, "args").expect("args") {
            NodeValue::Sequence(items) => items,
            _ => Vec::new(),
        };
        let inner = view_node(&args[0], &ModuleTable::EMPTY).expect("inner views");
        let NodeValue::Record(_, inner_fields) = &inner else {
            p.fail("inner-is-call", "the inner node must be a Call record");
            return;
        };
        let inner_args = match inner_fields.get("args") {
            Some(NodeValue::Sequence(items)) => items.clone(),
            _ => Vec::new(),
        };
        let literal = make_quoted(
            &node_record(
                "Literal",
                vec![("value".to_string(), NodeValue::Scalar(CodeValue::Int(5)))],
            ),
            &ModuleTable::EMPTY,
        )
        .expect("literal makes");
        let inner_bumped = node_record(
            "Call",
            vec![
                ("callee".to_string(), node_tag("Add")),
                (
                    "args".to_string(),
                    emath_rt::code_tree::node_list(vec![
                        inner_args[0].clone(),
                        NodeValue::Code(Box::new(literal)),
                    ]),
                ),
            ],
        );
        let inner_remade = make_quoted(&inner_bumped, &ModuleTable::EMPTY)
            .expect("the modified inner rebuilds");
        let outer_bumped = node_record(
            "Call",
            vec![
                ("callee".to_string(), node_tag("Mul")),
                (
                    "args".to_string(),
                    emath_rt::code_tree::node_list(vec![
                        NodeValue::Code(Box::new(inner_remade)),
                        NodeValue::Scalar(CodeValue::Int(4)),
                    ]),
                ),
            ],
        );
        let rebuilt = make_quoted(&outer_bumped, &ModuleTable::EMPTY).expect("modified rebuilds");
        let closed = substitute_expr(
            &substitute_expr(&rebuilt, "c", CodeValue::Int(1)),
            "x",
            CodeValue::Int(2),
        );
        p.eq(
            "modification-changes-value",
            evaluate_expr(&closed, &ModuleTable::EMPTY),
            Ok(CodeValue::Int(28)),
        );
    });

    // 5. The scalar tree evaluator: the VM's carrier rules over the
    //    same kernels - Int collapse, Rat lock, division never
    //    collapses, short-circuit combinators, the unbound refusal,
    //    and the named refusals for calls and globals.
    p.case("tree-evaluator-carrier-rules", |p| {
        let inputs = |names: &[(&str, CodeValue)]| {
            names
                .iter()
                .map(|(name, value)| (name.to_string(), value.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let int_sum = CodeTree::Binary {
            op: TreeBinary::Add,
            left: Box::new(CodeTree::Literal(CodeValue::Int(2))),
            right: Box::new(CodeTree::Literal(CodeValue::Int(3))),
        };
        p.eq(
            "int-plus-int-stays-int",
            evaluate_tree(&int_sum, &inputs(&[])),
            Ok(CodeValue::Int(5)),
        );
        let rat_zero = CodeTree::Binary {
            op: TreeBinary::Sub,
            left: Box::new(CodeTree::Literal(CodeValue::Rat((1, 4)))),
            right: Box::new(CodeTree::Literal(CodeValue::Rat((1, 4)))),
        };
        p.eq(
            "rational-zero-never-collapses",
            evaluate_tree(&rat_zero, &inputs(&[])),
            Ok(CodeValue::Rat((0, 1))),
        );
        let division = CodeTree::Binary {
            op: TreeBinary::Div,
            left: Box::new(CodeTree::Literal(CodeValue::Int(4))),
            right: Box::new(CodeTree::Literal(CodeValue::Int(2))),
        };
        p.eq(
            "division-stays-rational",
            evaluate_tree(&division, &inputs(&[])),
            Ok(CodeValue::Rat((2, 1))),
        );
        let short_circuit = CodeTree::Binary {
            op: TreeBinary::And,
            left: Box::new(CodeTree::Literal(CodeValue::Bool(false))),
            right: Box::new(CodeTree::Call {
                function: Box::new(CodeTree::Path(vec!["never".to_string()])),
                args: Vec::new(),
            }),
        };
        p.eq(
            "and-short-circuits",
            evaluate_tree(&short_circuit, &inputs(&[])),
            Ok(CodeValue::Bool(false)),
        );
        let open = CodeTree::Binary {
            op: TreeBinary::Add,
            left: Box::new(CodeTree::Path(vec!["x".to_string()])),
            right: Box::new(CodeTree::Literal(CodeValue::Int(1))),
        };
        match evaluate_tree(&open, &inputs(&[])) {
            Ok(_) => {
                p.fail("unbound-refuses", "an unbound name must refuse");
            }
            Err(fault) => {
                p.demand(
                    "named-refusal",
                    fault.contains("is not emitted in the scalar tree evaluator"),
                    &format!("the refusal must name the tree boundary: {fault}"),
                );
            }
        }
    });

    // 6. Substitution over the tree is by name, absent no-op; the
    //    free-name walk collects paths only; the dependency snapshot
    //    stamps resolving names against the module table and the
    //    stale check refuses a changed identity.
    p.case("tree-substitution-and-deps", |p| {
        let substituted = substitute_tree(
            &template_tree(),
            "c",
            &CodeTree::Literal(CodeValue::Int(5)),
        );
        p.eq(
            "free-names-after-substitute",
            free_names_tree(&substituted),
            vec!["x".to_string()],
        );
        let untouched = substitute_tree(
            &template_tree(),
            "zz",
            &CodeTree::Literal(CodeValue::Int(1)),
        );
        p.eq(
            "absent-substitute-no-op",
            untouched == template_tree(),
            true,
        );
        let table = ModuleTable {
            globals: &[("f", false)],
            stamps: &[("f", 42)],
        };
        let calls_f = CodeTree::Call {
            function: Box::new(CodeTree::Path(vec!["f".to_string()])),
            args: vec![CodeTree::Literal(CodeValue::Int(1))],
        };
        p.eq(
            "snapshot-stamps-resolving-names",
            dependency_snapshot_tree(&calls_f, &table),
            std::collections::BTreeMap::from([("f".to_string(), 42u64)]),
        );
        p.demand(
            "verified-deps-admit",
            verify_deps(&dependency_snapshot_tree(&calls_f, &table), &table).is_ok(),
            "a stamp that still resolves admits",
        );
        let changed = ModuleTable {
            globals: &[("f", false)],
            stamps: &[("f", 43)],
        };
        match verify_deps(&std::collections::BTreeMap::from([("f".to_string(), 42u64)]), &changed) {
            Ok(()) => {
                p.fail("stale-refuses", "a changed identity must refuse");
            }
            Err(fault) => {
                p.demand(
                    "stale-name",
                    fault == "stale_dependency: quoted dependency `f` changed since capture",
                    &format!("the stale refusal must match the VM's: {fault}"),
                );
            }
        }
    });

    // 7. Node family laws: tags are empty-field records, equality is
    //    structural with scalar VALUE equality (`2 == 2/1`), mixed
    //    kinds never equal, sequence indexing is checked, and a
    //    missing field refuses typed.
    p.case("node-family-laws", |p| {
        p.eq("tag-vs-tag", node_eq(&node_tag("Call"), &node_tag("Call")), Ok(true));
        p.eq(
            "tag-vs-other-tag",
            node_eq(&node_tag("Call"), &node_tag("Literal")),
            Ok(false),
        );
        p.eq(
            "scalar-value-equality",
            node_eq(
                &NodeValue::Scalar(CodeValue::Int(2)),
                &NodeValue::Scalar(CodeValue::Rat((2, 1))),
            ),
            Ok(true),
        );
        p.eq(
            "mixed-kinds-never-equal",
            node_eq(
                &NodeValue::Scalar(CodeValue::Int(1)),
                &node_tag("Call"),
            ),
            Ok(false),
        );
        let sequence = emath_rt::code_tree::node_list(vec![
            NodeValue::Scalar(CodeValue::Int(10)),
            NodeValue::Scalar(CodeValue::Int(20)),
        ]);
        p.eq(
            "checked-index",
            node_eq(
                &node_index(&sequence, 1).expect("in-range index"),
                &NodeValue::Scalar(CodeValue::Int(20)),
            ),
            Ok(true),
        );
        match node_index(&sequence, 5) {
            Ok(_) => {
                p.fail("index-bounds", "an out-of-range index must refuse");
            }
            Err(fault) => {
                p.demand(
                    "vm-index-message",
                    fault == "invalid_index: sequence index out of range",
                    &format!("the index refusal must match the VM's: {fault}"),
                );
            }
        }
        match node_field(&node_tag("Call"), "args") {
            Ok(_) => {
                p.fail("missing-field-refuses", "a tag has no fields");
            }
            Err(fault) => {
                p.demand(
                    "typed-field-refusal",
                    fault == "type: record has no field `args`",
                    &format!("the field refusal must be typed: {fault}"),
                );
            }
        }
    });

    // 8. Reconstruction refusals: node kinds outside the emitted
    //    subset refuse by name (the not-yet-emitted family), and the
    //    VM's reconstruction messages survive verbatim.
    p.case("reconstruction-boundaries", |p| {
        match rebuild_node(&node_record(
            "Call",
            vec![(
                "args".to_string(),
                emath_rt::code_tree::node_list(vec![]),
            )],
        )) {
            Ok(_) => {
                p.fail("missing-callee-refuses", "a Call without a callee must refuse");
            }
            Err(fault) => {
                p.demand(
                    "vm-call-message",
                    fault == "Call missing callee",
                    &format!("the reconstruction message must match the VM's: {fault}"),
                );
            }
        }
        match rebuild_node(&node_tag("Closure")) {
            Ok(_) => {
                p.fail("closure-refuses", "a Closure node must refuse in this cut");
            }
            Err(fault) => {
                p.demand(
                    "named-not-yet-emitted",
                    fault == "node `Closure` is not yet emitted in artifact trees",
                    &format!("the boundary refusal must be named: {fault}"),
                );
            }
        }
        // The unary op tag names survive: Neg views as a Call with a
        // `Neg` callee tag.
        let negated = view_tree(
            &CodeTree::Unary {
                op: TreeUnary::Neg,
                value: Box::new(CodeTree::Literal(CodeValue::Int(1))),
            },
            &ModuleTable::EMPTY,
        );
        p.eq(
            "unary-callee-tag",
            node_eq(
                &node_field(&negated, "callee").expect("callee"),
                &node_tag("Neg"),
            ),
            Ok(true),
        );
    });

    // 9. Definition-table laws (the quote.body unfold, bead
    //    emath-quote-body-defs-trto7): a transparent row yields
    //    Available with a dependency-free fragment that COMPUTES the
    //    body value; an opaque row yields the Opaque record with
    //    identity and signature tags; a name the table does not
    //    carry yields Opaque-unbound (a record, never a fault); a
    //    non-Path code yields Available of its own tree; a
    //    transparent row outside the distilled subset refuses by
    //    name.
    p.case("definition-table-laws", |p| {
        let defs = DefinitionTable {
            rows: vec![
                DefinitionRow {
                    name: "Z".to_string(),
                    opaque: false,
                    body: Some(CodeTree::Binary {
                        op: TreeBinary::Mul,
                        left: Box::new(CodeTree::Binary {
                            op: TreeBinary::Add,
                            left: Box::new(CodeTree::Literal(CodeValue::Int(1))),
                            right: Box::new(CodeTree::Literal(CodeValue::Int(1))),
                        }),
                        right: Box::new(CodeTree::Literal(CodeValue::Int(4))),
                    }),
                },
                DefinitionRow {
                    name: "O".to_string(),
                    opaque: true,
                    body: None,
                },
                DefinitionRow {
                    name: "U".to_string(),
                    opaque: false,
                    body: None,
                },
            ],
        };
        let named = |name: &str| {
            open_expr(
                vec![name.to_string()],
                CodeTree::Path(vec![name.to_string()]),
                None,
                std::collections::BTreeMap::new(),
            )
        };
        // The transparent unfold: Available with a computing fragment.
        let z = quote_body_node(&named("Z"), &defs).expect("transparent unfolds");
        p.eq(
            "transparent-kind",
            node_eq(&node_field(&z, "kind").expect("kind"), &node_tag("Available")),
            Ok(true),
        );
        let fragment = match node_field(&z, "fragment") {
            Ok(NodeValue::Code(code)) => *code,
            _ => {
                p.fail("transparent-fragment", "the fragment must be a Code value");
                return;
            }
        };
        p.eq(
            "fragment-computes-body-value",
            evaluate_expr(&fragment, &ModuleTable::EMPTY),
            Ok(CodeValue::Int(8)),
        );
        // The opaque unfold: Opaque with identity and signature tags.
        let o = quote_body_node(&named("O"), &defs).expect("opaque unfolds");
        p.eq(
            "opaque-kind",
            node_eq(&node_field(&o, "kind").expect("kind"), &node_tag("Opaque")),
            Ok(true),
        );
        p.eq(
            "opaque-identity",
            node_eq(&node_field(&o, "identity").expect("identity"), &node_tag("O")),
            Ok(true),
        );
        p.eq(
            "opaque-signature",
            node_eq(
                &node_field(&o, "signature").expect("signature"),
                &node_tag("opaque"),
            ),
            Ok(true),
        );
        // The unbound name: Opaque with the `unbound` signature - a
        // record, never a fault.
        let u = quote_body_node(&named("zz"), &defs).expect("unbound unfolds");
        p.eq(
            "unbound-kind",
            node_eq(&node_field(&u, "kind").expect("kind"), &node_tag("Opaque")),
            Ok(true),
        );
        p.eq(
            "unbound-signature",
            node_eq(
                &node_field(&u, "signature").expect("signature"),
                &node_tag("unbound"),
            ),
            Ok(true),
        );
        // A non-Path code yields Available of its own tree.
        let own = open_expr(
            Vec::new(),
            CodeTree::Binary {
                op: TreeBinary::Add,
                left: Box::new(CodeTree::Literal(CodeValue::Int(2))),
                right: Box::new(CodeTree::Literal(CodeValue::Int(3))),
            },
            None,
            std::collections::BTreeMap::new(),
        );
        let a = quote_body_node(&own, &defs).expect("non-path unfolds");
        p.eq(
            "non-path-kind",
            node_eq(&node_field(&a, "kind").expect("kind"), &node_tag("Available")),
            Ok(true),
        );
        let fragment = match node_field(&a, "fragment") {
            Ok(NodeValue::Code(code)) => *code,
            _ => {
                p.fail("non-path-fragment", "the fragment must be a Code value");
                return;
            }
        };
        p.eq(
            "non-path-fragment-computes",
            evaluate_expr(&fragment, &ModuleTable::EMPTY),
            Ok(CodeValue::Int(5)),
        );
        // A transparent row outside the distilled subset refuses by
        // name (the no-claim boundary).
        match quote_body_node(&named("U"), &defs) {
            Ok(_) => {
                p.fail("non-distilled-refuses", "an uncarried body must refuse");
            }
            Err(fault) => {
                p.demand(
                    "non-distilled-named",
                    fault == "tree: the body of `U` is not emitted in artifact trees",
                    &format!("the boundary refusal must be named: {fault}"),
                );
            }
        }
    });

    p.finish();
}

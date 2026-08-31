//! Temporary probe (removed before close): what the parser produces
//! for ch7 events/transitions bodies.
#[test]
fn probe_parse() {
    use emath_core::limits::Limits;
    use emath_sema::CompilerSession;
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    for (name, text) in [
        ("on-paren", "emath model m:\n    state:\n        heat: Float64\n    transitions:\n        on Bounce():\n            velocity = 0.0\n"),
        ("on-same-line", "emath model m:\n    state:\n        heat: Float64\n    transitions:\n        on Bounce: velocity = 0.0\n"),
        ("on-fn-head", "emath model m:\n    state:\n        heat: Float64\n    transitions:\n        on Bounce(velocity):\n            velocity = 0.0\n"),
    ] {
        let (tree, diagnostics) = session.parse_text(text);
        println!(
            "{name}: items={} errors={:?}",
            tree.items.len(),
            diagnostics
                .errors()
                .map(|d| format!("{}: {}", d.code, d.message))
                .collect::<Vec<_>>()
        );
        for item in &tree.items {
            if let emath_core::tree::Item::Declaration(decl) = item {
                for stmt in &decl.body {
                    if let emath_core::tree::StmtKind::Section(section) = &stmt.kind {
                        println!(
                            "  section `{}` stmts={} kinds={:?}",
                            section.name,
                            section.suite.statements.len(),
                            section
                                .suite
                                .statements
                                .iter()
                                .map(|s| match &s.kind {
                                    emath_core::tree::StmtKind::FnDecl { head, name, .. } => {
                                        format!("FnDecl({head},{name})")
                                    }
                                    emath_core::tree::StmtKind::Command { head, .. } => {
                                        format!("Command({head:?})")
                                    }
                                    emath_core::tree::StmtKind::Assign { .. } => "Assign".to_string(),
                                    other => format!("{other:?}").chars().take(24).collect(),
                                })
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}

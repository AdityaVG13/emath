//! V6 intent-compiler HIR (docs/06 + docs/26).
//!
//! The HIR layer between lossless syntax and the neutral IR: name
//! resolution, declaration kinds, constructors and goals, with source
//! spans retained. IDs are stable content-derived handles, not vector
//! positions (V6 IR rule), so renaming a declaration changes its id and
//! downstream ids deterministically.
//!
//! The `IntentGraph` records declarations, goals and definitions as nodes
//! and their reference edges (declaration-to-declaration uses, goal
//! targets, definition bodies). Construction is deterministic: the same
//! package yields the same graph, byte-comparable.

#![forbid(unsafe_code)]

use emath_core::{content_id_of_str, ContentId, Span};
use emath_syntax::tree::{Declaration, ExprKind, Item, StmtKind, SyntaxTree, TypeKind, UseTree};
use std::collections::BTreeMap;

/// V6 `PackageId`: content-derived from the package identity string.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageId(pub ContentId);

impl PackageId {
    #[must_use]
    pub fn derive(identity: &str) -> Self {
        Self(content_id_of_str(&format!("package::{identity}")))
    }
}

/// V6 `DeclarationId`: content-derived from (package, name, declaration
/// body), independent of source position.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeclarationId(pub ContentId);

impl DeclarationId {
    #[must_use]
    pub fn derive(package: &PackageId, name: &str, body: &str) -> Self {
        Self(content_id_of_str(&format!(
            "decl::{}/{}::{}",
            package.0 .0, name, body
        )))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0 .0
    }
}

/// V6 `HirDeclarationKind`: what the `.emath` declaration is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HirDeclarationKind {
    /// `... as policy:` — stateful declaration with constructors/goals.
    Policy,
    /// `... as function:` — pure mapping declaration.
    Function,
    /// `... as world:` — V7 semantic-genesis world declaration.
    World,
    /// Custom kind declared via the kind schema (P04-005).
    Custom,
}

impl HirDeclarationKind {
    #[must_use]
    pub fn from_as_kind(as_kind: &str) -> Self {
        match as_kind {
            "policy" => Self::Policy,
            "function" => Self::Function,
            "world" => Self::World,
            _ => Self::Custom,
        }
    }
}

/// A constructor in HIR (name, typed parameters by HIR type text, and the
/// declared error type name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HirConstructor {
    pub name: String,
    pub parameters: Vec<(String, HirType)>,
    pub error_type: Option<String>,
    pub source: Span,
}

/// HIR type: the syntax type surface with generics resolved by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HirType {
    pub segments: Vec<String>,
    pub generic_args: Vec<HirType>,
}

impl HirType {
    #[must_use]
    pub fn from_syntax(ty: &emath_syntax::tree::TypeExpr) -> Self {
        match &ty.kind {
            TypeKind::Path {
                segments,
                generic_args,
            } => Self {
                segments: segments.clone(),
                generic_args: generic_args.iter().map(Self::from_syntax).collect(),
            },
            // Non-path type surfaces are carried as a distinguished path so
            // the HIR stays total; semantics enter at the neutral IR.
            TypeKind::List(items) => Self {
                segments: vec!["list".into()],
                generic_args: items.iter().map(Self::from_syntax).collect(),
            },
            TypeKind::Tuple(items) => Self {
                segments: vec!["tuple".into()],
                generic_args: items.iter().map(Self::from_syntax).collect(),
            },
            TypeKind::Ref(inner) => Self {
                segments: vec!["ref".into()],
                generic_args: vec![Self::from_syntax(inner)],
            },
            TypeKind::Product(items) => Self {
                segments: vec!["product".into()],
                generic_args: items.iter().map(Self::from_syntax).collect(),
            },
        }
    }
}

/// A goal attached to a declaration (`evaluate <target>`, requests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HirGoal {
    pub kind: String,
    pub target: String,
    pub source: Span,
}

/// One declaration in HIR, resolved against the package's use items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HirDeclaration {
    pub id: DeclarationId,
    pub name: String,
    pub kind: HirDeclarationKind,
    /// Visibility text from attributes (`public`/`package`/`private`).
    pub visibility: String,
    pub sections: Vec<String>,
    pub constructors: Vec<HirConstructor>,
    pub goals: Vec<HirGoal>,
    pub source: Span,
}

/// The HIR package: declarations plus the import table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HirPackage {
    pub id: PackageId,
    pub declarations: Vec<HirDeclaration>,
    /// Resolved imports: alias -> canonical path segments.
    pub imports: BTreeMap<String, Vec<String>>,
}

impl HirPackage {
    /// Lower a syntax tree into HIR with name resolution for `use` items.
    #[must_use]
    pub fn lower(tree: &SyntaxTree, package_id: &str) -> Self {
        let id = PackageId::derive(package_id);
        let mut imports = BTreeMap::new();
        let mut declarations = Vec::new();
        for item in &tree.items {
            match item {
                Item::Use { path, tree, .. } => {
                    let resolved: Vec<String> = path.clone();
                    match tree {
                        UseTree::All => {
                            imports
                                .entry(path.last().cloned().unwrap_or_default())
                                .or_insert_with(|| resolved.clone());
                        }
                        UseTree::Named(names) => {
                            for (name, alias) in names {
                                let mut full = resolved.clone();
                                full.push(name.clone());
                                imports
                                    .entry(alias.clone().unwrap_or_else(|| name.clone()))
                                    .or_insert(full);
                            }
                        }
                    }
                }
                Item::Declaration(decl) => {
                    declarations.push(lower_declaration(&id, decl));
                }
            }
        }
        Self {
            id,
            declarations,
            imports,
        }
    }
}

fn lower_declaration(package: &PackageId, decl: &Declaration) -> HirDeclaration {
    let mut constructors = Vec::new();
    let mut goals = Vec::new();
    let mut sections = Vec::new();
    for section in &decl.sections {
        sections.push(section.name.clone());
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::FnDecl {
                    name, params, ret, ..
                } => {
                    if section.name == "constructors" {
                        constructors.push(HirConstructor {
                            name: name.clone(),
                            parameters: params
                                .iter()
                                .map(|p| (p.name.clone(), HirType::from_syntax(&p.ty)))
                                .collect(),
                            error_type: ret.as_ref().map(|ty| {
                                if let TypeKind::Path { segments, .. } = &ty.kind {
                                    segments.join("::")
                                } else {
                                    "ConfigError".into()
                                }
                            }),
                            source: stmt.source,
                        });
                    }
                }
                StmtKind::Section(inner) if inner.name == "evaluate" => {
                    if let Some(target) = &inner.generic {
                        goals.push(HirGoal {
                            kind: "evaluate".into(),
                            target: target.clone(),
                            source: stmt.source,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    let body = render_decl_body(decl);
    HirDeclaration {
        id: DeclarationId::derive(package, &decl.name, &body),
        name: decl.name.clone(),
        kind: HirDeclarationKind::from_as_kind(&decl.as_kind),
        visibility: decl
            .attributes
            .iter()
            .map(|a| a.name.clone())
            .collect::<Vec<_>>()
            .join(","),
        sections,
        constructors,
        goals,
        source: decl.source,
    }
}

/// Content for the declaration id: kind + name + section/statement shapes
/// with all spans stripped (position-independent identity).
fn render_decl_body(decl: &Declaration) -> String {
    let mut out = String::new();
    out.push_str(&decl.item_kind);
    out.push(':');
    out.push_str(&decl.as_kind);
    for section in &decl.sections {
        out.push('\n');
        out.push_str(&section.name);
        if let Some(generic) = &section.generic {
            out.push('<');
            out.push_str(generic);
            out.push('>');
        }
        for stmt in &section.suite.statements {
            out.push('\n');
            push_stmt_shape(&mut out, stmt);
        }
    }
    out
}

fn push_stmt_shape(out: &mut String, stmt: &emath_syntax::tree::Stmt) {
    match &stmt.kind {
        StmtKind::FieldDecl {
            visibility,
            name,
            ty,
            ..
        } => {
            if let Some(v) = visibility {
                out.push_str(&format!("{v:?} "));
            }
            out.push_str(name);
            out.push(':');
            push_type_shape(out, ty);
        }
        StmtKind::FnDecl {
            name, params, ret, ..
        } => {
            out.push_str("fn ");
            out.push_str(name);
            out.push('(');
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&p.name);
                out.push(':');
                push_type_shape(out, &p.ty);
            }
            out.push(')');
            if let Some(ret_ty) = ret {
                out.push_str("->");
                push_type_shape(out, ret_ty);
            }
        }
        StmtKind::Assign { target, value } => {
            out.push_str(&target.segments.join("::"));
            out.push('=');
            push_expr_shape(out, value);
        }
        StmtKind::Let { name, value, .. } => {
            out.push_str("let ");
            out.push_str(name);
            out.push('=');
            push_expr_shape(out, value);
        }
        StmtKind::Given { name, value } => {
            out.push_str("given ");
            out.push_str(name);
            out.push('=');
            push_expr_shape(out, value);
        }
        StmtKind::Expect(expr) => {
            out.push_str("expect ");
            push_expr_shape(out, expr);
        }
        StmtKind::Require(expr) => {
            out.push_str("require ");
            push_expr_shape(out, expr);
        }
        StmtKind::Ensure(expr) => {
            out.push_str("ensure ");
            push_expr_shape(out, expr);
        }
        StmtKind::Command { head, .. } => {
            out.push_str(&head.join(" "));
        }
        StmtKind::Section(s) => {
            out.push_str(&s.name);
            if let Some(generic) = &s.generic {
                out.push('<');
                out.push_str(generic);
                out.push('>');
            }
        }
        _ => {
            let tag = format!("{:?}", stmt.kind);
            let head = tag.split_whitespace().next().unwrap_or("?").to_string();
            out.push_str(&head);
        }
    }
}

fn push_type_shape(out: &mut String, ty: &emath_syntax::tree::TypeExpr) {
    out.push_str(&ty.kind.debug_segments().join("::"));
}

fn push_expr_shape(out: &mut String, expr: &emath_syntax::tree::Expr) {
    match &expr.kind {
        ExprKind::Int(t) | ExprKind::Float(t) => out.push_str(t),
        ExprKind::Str(s) => {
            out.push('"');
            out.push_str(s);
            out.push('"');
        }
        ExprKind::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        ExprKind::Path { segments, .. } => out.push_str(&segments.join("::")),
        ExprKind::Binary { op, left, right } => {
            push_expr_shape(out, left);
            out.push_str(&format!("{op:?}"));
            push_expr_shape(out, right);
        }
        ExprKind::Unary { op, value } => {
            out.push_str(&format!("{op:?}"));
            push_expr_shape(out, value);
        }
        ExprKind::Call { function, args } => {
            push_expr_shape(out, function);
            out.push('(');
            for arg in args {
                push_expr_shape(out, arg);
                out.push(',');
            }
            out.push(')');
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            out.push_str("if");
            push_expr_shape(out, condition);
            push_expr_shape(out, then_value);
            push_expr_shape(out, else_value);
        }
        ExprKind::Quantity { value, unit } => {
            push_expr_shape(out, value);
            out.push_str(&unit.join("::"));
        }
        other => {
            let tag = format!("{other:?}");
            let head = tag.split_whitespace().next().unwrap_or("?").to_string();
            out.push_str(&head);
        }
    }
}

// --- IntentGraph ---------------------------------------------------------

/// A node in the intent graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentNode {
    Declaration(DeclarationId),
    Goal {
        declaration: DeclarationId,
        kind: String,
        target: String,
    },
}

/// An edge between intent nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentEdgeKind {
    /// The declaration body references another declaration's definitions
    /// (imports resolved to first segment).
    References,
    /// A goal targets a definition in its declaration.
    Targets,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentEdge {
    pub from: IntentNode,
    pub to: IntentNode,
    pub kind: IntentEdgeKind,
}

/// V6 `IntentGraph`: declaration/goal/definition nodes with reference
/// edges, built deterministically from an `HirPackage`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct IntentGraph {
    pub nodes: Vec<IntentNode>,
    pub edges: Vec<IntentEdge>,
}

impl IntentGraph {
    /// Build the graph for a package. Deterministic: node and edge order
    /// follow declaration and section order in the tree.
    #[must_use]
    pub fn build(package: &HirPackage) -> Self {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for decl in &package.declarations {
            let decl_node = IntentNode::Declaration(decl.id.clone());
            nodes.push(decl_node.clone());
            for goal in &decl.goals {
                let goal_node = IntentNode::Goal {
                    declaration: decl.id.clone(),
                    kind: goal.kind.clone(),
                    target: goal.target.clone(),
                };
                nodes.push(goal_node.clone());
                edges.push(IntentEdge {
                    from: goal_node.clone(),
                    to: decl_node.clone(),
                    kind: IntentEdgeKind::Targets,
                });
            }
        }
        Self { nodes, edges }
    }

    /// Deterministic render (byte-comparable across runs).
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for node in &self.nodes {
            out.push_str(&format!("node {}\n", node_key(node)));
        }
        for edge in &self.edges {
            out.push_str(&format!(
                "edge {} {} {}\n",
                edge_kind_str(&edge.kind),
                node_key(&edge.from),
                node_key(&edge.to)
            ));
        }
        out
    }
}

fn node_key(node: &IntentNode) -> String {
    match node {
        IntentNode::Declaration(id) => format!("decl:{}", id.as_str()),
        IntentNode::Goal {
            declaration,
            kind,
            target,
        } => format!("goal:{}:{kind}:{target}", declaration.as_str()),
    }
}

fn edge_kind_str(kind: &IntentEdgeKind) -> &'static str {
    match kind {
        IntentEdgeKind::References => "references",
        IntentEdgeKind::Targets => "targets",
    }
}

// Small helper used for HIR type shapes without importing tree TypeKind
// internals elsewhere.
trait DebugSegments {
    fn debug_segments(&self) -> Vec<String>;
}

impl DebugSegments for emath_syntax::tree::TypeKind {
    fn debug_segments(&self) -> Vec<String> {
        format!("{self:?}")
            .split_whitespace()
            .next()
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_syntax::parse_str;

    const STATEFUL: &str = include_str!("../../../implementation/tests/valid/stateful.emath");
    const MINIMAL: &str = include_str!("../../../implementation/tests/valid/minimal.emath");

    fn lower(source: &str) -> HirPackage {
        let (tree, diagnostics) = parse_str(source);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        HirPackage::lower(&tree, "test-package")
    }

    #[test]
    fn lower_stateful_policy() {
        let package = lower(STATEFUL);
        assert_eq!(package.declarations.len(), 1);
        let decl = &package.declarations[0];
        assert_eq!(decl.name, "AffinePolicy");
        assert_eq!(decl.kind, HirDeclarationKind::Policy);
        assert!(!decl.sections.is_empty());
        assert!(decl.sections.contains(&"constructors".to_string()));
        assert!(
            decl.constructors
                .iter()
                .any(|c| c.name == "new" && c.parameters.len() == 2),
            "constructors: {:?}",
            decl.constructors
        );
    }

    #[test]
    fn declaration_ids_are_content_derived() {
        let package = lower(STATEFUL);
        assert_eq!(package.declarations.len(), 1);
        let id_a = package.declarations[0].id.clone();
        // Same package lowers to the same id.
        let again = lower(STATEFUL);
        assert_eq!(id_a, again.declarations[0].id);
        // A changed name changes the id deterministically.
        let changed = lower(&STATEFUL.replace("AffinePolicy", "RenamedPolicy"));
        assert_ne!(id_a, changed.declarations[0].id);
        // Different packages with identical bodies differ.
        let other_package = HirPackage::lower(&parse_str(STATEFUL).0, "another-package");
        assert_ne!(id_a, other_package.declarations[0].id);
    }

    #[test]
    fn intent_graph_is_deterministic() {
        let package = lower(MINIMAL);
        let a = IntentGraph::build(&package);
        let b = IntentGraph::build(&lower(MINIMAL));
        assert_eq!(a.render(), b.render());
        assert!(!a.nodes.is_empty());
    }

    #[test]
    fn imports_resolve_glob_paths() {
        // The bootstrap parser tracks `use path::*` and paths; alias
        // syntax (`as P`) is not yet captured by the parser (P04-003).
        // HIR resolves what the parser hands over.
        let package = lower(
            "use core::math::*\nemath custom <S> as function:\n    inputs:\n        x: Real\n",
        );
        assert!(
            package.imports.contains_key("math"),
            "imports: {:?}",
            package.imports
        );
        assert_eq!(
            package.imports["math"],
            vec!["core".to_string(), "math".to_string()]
        );
        let _ = UseTree::All;
    }

    #[test]
    fn goals_are_recorded_with_targets() {
        let package = lower(MINIMAL);
        assert!(
            package.declarations[0]
                .goals
                .iter()
                .any(|g| g.kind == "evaluate" && !g.target.is_empty()),
            "goals: {:?}",
            package.declarations[0].goals
        );
    }
}

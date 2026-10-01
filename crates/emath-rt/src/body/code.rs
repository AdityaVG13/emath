// The artifact-side Code value: a quoted unary program compiled once
// into a closure factory. `substitute` binds one open constant by
// partial application; `evaluate` demands closure over the open set
// (open code refuses `unbound_code`, naming every remaining open
// constant). No tree is carried and no interpreter runs: candidates
// execute as the compiled closures the backend emitted.
//
// The carrier is generic: the backend instantiates `Code<V>` over the
// template's declared scalar domain (`Code<ExactRatio>` for Rat,
// `Code<i64>` for Int, `Code<bool>` for Bool). The carrier governs the
// parameter and the open constants together - a quoted Int program is
// an Int program - so the compiled factory stays monomorphic and
// cross-lane parity holds on the admitted subset.
//
// Carrier contract: `free` is the binding order the factory consumes;
// `substitute` removes the bound name from `free` and splices the
// value at its position when forwarding; binding a name that is not
// open is a no-op (tree-substitution parity: substituting an absent
// reference leaves the code unchanged); `evaluate` refuses while any
// name remains open, naming them all in binding order.

use std::rc::Rc;

use super::{ratio_add, ratio_div, ratio_mul, ratio_sub, ExactRatio};

/// The specialized unary program: a function over the carrier.
pub type Unary<V> = Rc<dyn Fn(V) -> Result<V, String>>;

/// The template factory: consumes the open constants' values (in
/// `free` order) and yields the specialized unary program.
pub type CodeFactory<V> = Rc<dyn Fn(&[V]) -> Result<Unary<V>, String>>;

/// A quoted program template with its open constants.
pub struct Code<V: Clone + 'static> {
    free: Vec<String>,
    /// The distilled tree the literal carried (the dual
    /// representation the union lane already carries - the
    /// view/make seam reads structure through it). Substitution
    /// splices the bound constant into it as a literal. `None` when
    /// the template has no distilled tree (Float64 carriers).
    tree: Option<super::code_tree::CodeTree>,
    make: CodeFactory<V>,
}

impl<V: Clone + 'static> Clone for Code<V> {
    fn clone(&self) -> Self {
        Code {
            free: self.free.clone(),
            tree: self.tree.clone(),
            make: Rc::clone(&self.make),
        }
    }
}

/// Open a compiled template: `free` names the open constants in the
/// order the factory consumes their values.
pub fn open<V: Clone + 'static>(free: Vec<String>, make: CodeFactory<V>) -> Code<V> {
    Code { free, tree: None, make }
}

/// Open a compiled template WITH its distilled tree: the literal
/// lane carries both representations so `quote.view` can open a
/// program template's structure; `substitute` keeps both in step.
pub fn open_with_tree<V: Clone + 'static>(
    free: Vec<String>,
    tree: super::code_tree::CodeTree,
    make: CodeFactory<V>,
) -> Code<V> {
    Code { free, tree: Some(tree), make }
}

impl<V: Clone + 'static> Code<V> {
    /// `quote.view` over a program template: the distilled tree the
    /// literal carried. A template without one refuses by name.
    pub fn view(
        &self,
        module: &super::code_tree::ModuleTable,
    ) -> Result<super::code_tree::NodeValue, String> {
        match &self.tree {
            Some(tree) => Ok(super::code_tree::view_tree(tree, module)),
            None => Err(String::from(
                "type: quote.view over a program value without a tree (a Float64 template)",
            )),
        }
    }

    /// `quote.identity` over a program template's tree; refuses like
    /// `view` when there is none.
    pub fn identity(&self) -> Result<i64, String> {
        match &self.tree {
            Some(tree) => super::code_tree::tree_identity(tree),
            None => Err(String::from(
                "type: quote.identity over a program value without a tree (a Float64 template)",
            )),
        }
    }
}

/// A template carrier's value as a tree literal. Float64 has no exact
/// tree literal (its templates carry no tree), so it answers `None`.
pub trait TreeScalar {
    fn tree_literal(&self) -> Option<CodeValue>;
}

impl TreeScalar for i64 {
    fn tree_literal(&self) -> Option<CodeValue> {
        Some(CodeValue::Int(*self))
    }
}

impl TreeScalar for ExactRatio {
    fn tree_literal(&self) -> Option<CodeValue> {
        Some(CodeValue::Rat(*self))
    }
}

impl TreeScalar for bool {
    fn tree_literal(&self) -> Option<CodeValue> {
        Some(CodeValue::Bool(*self))
    }
}

impl TreeScalar for f64 {
    fn tree_literal(&self) -> Option<CodeValue> {
        None
    }
}

impl<V: Clone + 'static> Code<V> {
    /// The template as a plain Code value (a `Code` record field, a
    /// `Code` parameter): its tree with the same open names. The
    /// compiled factory does not cross; the tree evaluator runs it,
    /// with the same carrier laws and the closure's domain checks.
    pub fn to_expr_code(&self) -> Result<ExprCode, String> {
        match &self.tree {
            Some(tree) => Ok(open_expr(
                self.free.clone(),
                tree.clone(),
                None,
                std::collections::BTreeMap::new(),
            )),
            None => Err(String::from(
                "type: a program value without a tree (a Float64 template) cannot be stored as Code",
            )),
        }
    }
}

/// Bind one open constant by partial application. The tree, when
/// present, takes the value as a literal in the same step, so the
/// substituted template still views, identifies, and stores as the
/// VM's substituted code.
pub fn substitute<V: Clone + TreeScalar + 'static>(
    code: &Code<V>,
    reference: &str,
    value: V,
) -> Code<V> {
    let Some(position) = code.free.iter().position(|name| name == reference) else {
        return code.clone();
    };
    let mut free = code.free.clone();
    free.remove(position);
    let inner = Rc::clone(&code.make);
    let bound = value.clone();
    let make = Rc::new(move |rest: &[V]| {
        let mut all = rest[..position].to_vec();
        all.push(bound.clone());
        all.extend_from_slice(&rest[position..]);
        inner(&all)
    });
    let tree = match (&code.tree, value.tree_literal()) {
        (Some(tree), Some(literal)) => Some(super::code_tree::substitute_tree(
            tree,
            reference,
            &super::code_tree::CodeTree::Literal(literal),
        )),
        _ => None,
    };
    Code { free, tree, make }
}

/// The guarded executor: closed code yields the specialized unary
/// program; open code refuses, naming every remaining constant.
pub fn evaluate<V: Clone + 'static>(code: &Code<V>) -> Result<Unary<V>, String> {
    if !code.free.is_empty() {
        return Err(format!(
            "unbound_code: quoted code is open; unbound name(s): {}",
            code.free.join(", ")
        ));
    }
    (code.make)(&[])
}

/// The open constant names in binding order.
pub fn free_names<V: Clone + 'static>(code: &Code<V>) -> &[String] {
    &code.free
}

// ---------------------------------------------------------------------------
// Expression templates (bead emath-expression-quotes-324y0): the
// quoted program is an EXPRESSION with free names, not a wrapped
// function - no parameter, no closure. The carrier of each free name
// is a substitute-time fact (the same template serves Int and Rat
// values), so the body computes over a value union and every scalar
// operation renders as a call to a dynamic kernel below.
//
// The kernels implement the constructor VM's EXACT carrier rules
// (normative source: `constructor_layer/ops.rs` — `binary`, `neg`,
// `eq_values`, `cmp_numeric`; the And/Or short-circuit arm in
// `engine_step/core.rs`):
//   - Int x Int keeps Int for `+ - *` when the canonical denominator
//     is 1 (`2 + 3` is Int 5, never Rat 5/1);
//   - any Rational operand locks the Rat carrier forever, even when
//     the result is integer-valued (`1/4 - 1/4` is Rat 0/1);
//   - division NEVER collapses to Int (`4 / 2` is Rat 2/1);
//   - a zero denominator refuses `division_by_zero`;
//   - numeric equality compares VALUES by cross-multiplication
//     (`2 == 2/1` is true), Bool equality is structural, and mixed
//     kinds are never equal (the VM's structural default);
//   - checked projections at typed boundaries mirror the engine's
//     `type_admits`: Rat-declared outputs widen Int exactly
//     (`n` becomes `n/1`), Int-declared outputs refuse a Rational
//     (`output `x` does not have the declared type, found 5/1`), Bool
//     admits Bool only.
//
// Carrier-width law: the VM's Int is arbitrary-precision `ExactInt`;
// this union's Int is checked i64 (the existing cross-lane width
// distinction). Parity holds in the i64-shared domain; beyond-i64
// values refuse here exactly as the Int emission lane always has.
//
// Dual representation (bead emath-shared-tree-view-make-bp8nu): the
// value additionally carries the DISTILLED TREE of the quoted
// expression plus its stamped dependencies, and the compiled factory
// becomes optional (a made tree has no compiled body). Both halves
// are emitted by the same lowering pass from the same authored
// quote, so they cannot drift; neither is hand-written. The tree is
// what `quote.view` walks and `quote.make` rebuilds; a made (or
// factory-less) code evaluates through the shared scalar tree
// evaluator over the same kernels below.

/// The dynamic scalar union: the carrier of a free name in an
/// expression template, and of its computed result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeValue {
    Int(i64),
    Rat(ExactRatio),
    Bool(bool),
}

impl core::fmt::Display for CodeValue {
    /// The engine's value spellings, so typed-boundary refusals match
    /// the VM's message text: `5`, `5/1`, `true`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CodeValue::Int(n) => write!(f, "{n}"),
            CodeValue::Rat((num, den)) => write!(f, "{num}/{den}"),
            CodeValue::Bool(b) => write!(f, "{b}"),
        }
    }
}

fn as_ratio(value: &CodeValue) -> Result<ExactRatio, String> {
    match value {
        CodeValue::Int(n) => Ok((i128::from(*n), 1)),
        CodeValue::Rat(ratio) => Ok(*ratio),
        CodeValue::Bool(_) => Err(format!("type: cannot treat {value} as a number")),
    }
}

/// Union addition under the VM's carrier rules. The Int-collapse law
/// (`2 + 3` is Int 5) lives in the both-Int fast paths below; every
/// result that reaches the rational path is Rat-locked forever (a
/// Rational operand locks the carrier - the VM's `integer_operands`
/// guard can never fire there, a probe-proven dead branch deleted).
pub fn code_add(left: &CodeValue, right: &CodeValue) -> Result<CodeValue, String> {
    if let (CodeValue::Int(a), CodeValue::Int(b)) = (left, right) {
        let sum = a
            .checked_add(*b)
            .ok_or_else(|| format!("overflow: {a} + {b} exceeds the Int carrier"))?;
        return Ok(CodeValue::Int(sum));
    }
    let (ln, ld) = as_ratio(left)?;
    let (rn, rd) = as_ratio(right)?;
    Ok(CodeValue::Rat(ratio_add((ln, ld), (rn, rd))?))
}

/// Union subtraction under the VM's carrier rules (see `code_add` for
/// the carrier-locking law).
pub fn code_sub(left: &CodeValue, right: &CodeValue) -> Result<CodeValue, String> {
    if let (CodeValue::Int(a), CodeValue::Int(b)) = (left, right) {
        let difference = a
            .checked_sub(*b)
            .ok_or_else(|| format!("overflow: {a} - {b} exceeds the Int carrier"))?;
        return Ok(CodeValue::Int(difference));
    }
    let (ln, ld) = as_ratio(left)?;
    let (rn, rd) = as_ratio(right)?;
    Ok(CodeValue::Rat(ratio_sub((ln, ld), (rn, rd))?))
}

/// Union multiplication under the VM's carrier rules (see `code_add`
/// for the carrier-locking law).
pub fn code_mul(left: &CodeValue, right: &CodeValue) -> Result<CodeValue, String> {
    if let (CodeValue::Int(a), CodeValue::Int(b)) = (left, right) {
        let product = a
            .checked_mul(*b)
            .ok_or_else(|| format!("overflow: {a} * {b} exceeds the Int carrier"))?;
        return Ok(CodeValue::Int(product));
    }
    let (ln, ld) = as_ratio(left)?;
    let (rn, rd) = as_ratio(right)?;
    Ok(CodeValue::Rat(ratio_mul((ln, ld), (rn, rd))?))
}

/// Union division: ALWAYS Rational (`4 / 2` is Rat 2/1), refusing a
/// zero denominator by name.
pub fn code_div(left: &CodeValue, right: &CodeValue) -> Result<CodeValue, String> {
    let (ln, ld) = as_ratio(left)?;
    let (rn, rd) = as_ratio(right)?;
    if rn == 0 {
        return Err(String::from("division_by_zero: exact division by zero"));
    }
    Ok(CodeValue::Rat(ratio_div((ln, ld), (rn, rd))?))
}

/// Union negation: checked on both numeric carriers.
pub fn code_neg(value: &CodeValue) -> Result<CodeValue, String> {
    match value {
        CodeValue::Int(n) => n
            .checked_neg()
            .map(CodeValue::Int)
            .ok_or_else(|| format!("overflow: -{n} exceeds the Int carrier")),
        CodeValue::Rat((num, den)) => num
            .checked_neg()
            .map(|num| CodeValue::Rat((num, *den)))
            .ok_or_else(|| format!("overflow: -{num}/{den} exceeds the rational carrier")),
        CodeValue::Bool(_) => Err(format!("type: cannot negate {value}")),
    }
}

/// Union numeric ordering: exact cross-multiplication (a finite
/// binary64 is an exact rational; this union has no float lane).
pub fn code_cmp(left: &CodeValue, right: &CodeValue) -> Result<core::cmp::Ordering, String> {
    let (ln, ld) = as_ratio(left)?;
    let (rn, rd) = as_ratio(right)?;
    ln.checked_mul(rd).and_then(|cross| rn.checked_mul(ld).map(|other| cross.cmp(&other)))
        .ok_or_else(|| String::from("overflow: exact comparison cross-product"))
}

/// Union equality: numeric pairs compare VALUES (`2 == 2/1` is
/// true), Bool pairs are structural, mixed kinds are never equal
/// (the VM's structural default).
pub fn code_eq(left: &CodeValue, right: &CodeValue) -> Result<bool, String> {
    match (left, right) {
        (CodeValue::Bool(a), CodeValue::Bool(b)) => Ok(a == b),
        (CodeValue::Bool(_), _) | (_, CodeValue::Bool(_)) => Ok(false),
        _ => Ok(code_cmp(left, right)?.is_eq()),
    }
}

/// Union boolean-combinator admission: the VM's message for a
/// non-Bool operand reaching `and`/`or`.
pub fn code_as_bool(value: &CodeValue) -> Result<bool, String> {
    match value {
        CodeValue::Bool(b) => Ok(*b),
        other => Err(format!("type: boolean combinator expects Bool, found {other}")),
    }
}

/// Union `not`: Bool only, the VM's message otherwise.
pub fn code_not(value: &CodeValue) -> Result<bool, String> {
    match value {
        CodeValue::Bool(b) => Ok(!b),
        other => Err(format!("type: not expects Bool, found {other}")),
    }
}

/// Checked projection to a declared Int output: the engine's
/// `type_admits` law — Int admits Int only (a Rational refuses by
/// name, never truncates).
pub fn project_i64(value: &CodeValue, output: &str) -> Result<i64, String> {
    match value {
        CodeValue::Int(n) => Ok(*n),
        other => Err(format!(
            "type: output `{output}` does not have the declared type, found {other}"
        )),
    }
}

/// Checked projection to a declared Rat output: the engine's
/// Int-widening law — `n` becomes `n/1` exactly; Bool refuses.
pub fn project_ratio(value: &CodeValue, output: &str) -> Result<ExactRatio, String> {
    match value {
        CodeValue::Int(n) => Ok((i128::from(*n), 1)),
        CodeValue::Rat(ratio) => Ok(*ratio),
        other => Err(format!(
            "type: output `{output}` does not have the declared type, found {other}"
        )),
    }
}

/// Checked projection to a declared Bool output: Bool admits Bool
/// only.
pub fn project_bool(value: &CodeValue, output: &str) -> Result<bool, String> {
    match value {
        CodeValue::Bool(b) => Ok(*b),
        other => Err(format!(
            "type: output `{output}` does not have the declared type, found {other}"
        )),
    }
}

/// Checked projection to an exact-integer operand slot: Int admits
/// (widening into the checked machine carrier); Rational and Bool
/// refuse by name - the same `type_admits` law as the output
/// projections, carried by the exact tier.
pub fn project_exact_int(value: &CodeValue, slot: &str) -> Result<super::exact_int::ExactInt, String> {
    match value {
        CodeValue::Int(n) => Ok(super::exact_int::ExactInt::from(*n)),
        other => Err(format!(
            "type: exact-integer slot `{slot}` does not have the declared type, found {other}"
        )),
    }
}

/// The compiled expression body: consumes the free names' values (in
/// `free` order) and computes the union-valued expression.
pub type ExprFactory = Rc<dyn Fn(&[CodeValue]) -> Result<CodeValue, String>>;

/// An open expression template with its free names. The dual
/// representation: the distilled tree (data - what view walks and
/// make rebuilds) plus the compiled factory (the fast path a static
/// template literal carries; a made tree has none), with the
/// capture-time dependency snapshot.
pub struct ExprCode {
    free: Vec<String>,
    pub(crate) tree: super::code_tree::CodeTree,
    pub(crate) make: Option<ExprFactory>,
    pub(crate) deps: std::collections::BTreeMap<String, u64>,
}

impl Clone for ExprCode {
    fn clone(&self) -> Self {
        ExprCode {
            free: self.free.clone(),
            tree: self.tree.clone(),
            make: self.make.clone(),
            deps: self.deps.clone(),
        }
    }
}

impl core::fmt::Debug for ExprCode {
    /// The diagnostic shape: the open names, the distilled tree, and
    /// the stamped dependencies (the compiled factory is a function
    /// value - it reports its presence, never its body).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExprCode")
            .field("free", &self.free)
            .field("tree", &self.tree)
            .field("make", &self.make.is_some())
            .field("deps", &self.deps)
            .finish()
    }
}

impl PartialEq for ExprCode {
    /// The node family's code law (`node_eq`): two codes are equal when
    /// their distilled trees and stamped dependencies are. The compiled
    /// factory is derived from the same tree, so it carries no identity
    /// of its own; records with code fields compare through this.
    fn eq(&self, other: &Self) -> bool {
        self.tree == other.tree && self.deps == other.deps
    }
}

/// Open an expression template: `free` names the expression's free
/// names in the order the factory consumes their values, `tree` is
/// the distilled tree, `make` the compiled factory when one exists,
/// `deps` the capture-time dependency snapshot.
pub fn open_expr(
    free: Vec<String>,
    tree: super::code_tree::CodeTree,
    make: Option<ExprFactory>,
    deps: std::collections::BTreeMap<String, u64>,
) -> ExprCode {
    ExprCode {
        free,
        tree,
        make,
        deps,
    }
}

/// Bind one free name by partial application (the same law as the
/// function-template `substitute`: by-name splice at the position,
/// an absent reference is a no-op). The binding is authoritative on
/// the TREE (a later view sees the substituted tree, exactly as the
/// VM's tree substitution does) and splices the compiled factory in
/// lock step when one exists.
pub fn substitute_expr(
    code: impl std::borrow::Borrow<ExprCode>,
    reference: &str,
    value: CodeValue,
) -> ExprCode {
    let code = code.borrow();
    let Some(position) = code.free.iter().position(|name| name == reference) else {
        return code.clone();
    };
    let mut free = code.free.clone();
    free.remove(position);
    let tree = super::code_tree::substitute_tree(
        &code.tree,
        reference,
        &super::code_tree::CodeTree::Literal(value),
    );
    let make: Option<ExprFactory> = code.make.clone().map(|inner| {
        let spliced: ExprFactory = Rc::new(move |rest: &[CodeValue]| {
            let mut all = rest[..position].to_vec();
            all.push(value);
            all.extend_from_slice(&rest[position..]);
            inner(&all)
        });
        spliced
    });
    ExprCode {
        free,
        tree,
        make,
        deps: code.deps.clone(),
    }
}

/// Substitute a whole made subtree into an expression template (the
/// VM's `value_to_expr` law: any replacement value rebuilds into the
/// replacement tree; a node-record value arrives here through
/// `quote.make`). The reference must be open (an absent name is the
/// no-op parity). A structural replacement cannot splice the
/// compiled factory - the factory drops and evaluation falls to the
/// shared scalar tree evaluator, the same lane `quote.make` results
/// take. Open names recompute over the spliced tree (the replacement
/// may itself carry open names) and the dependency snapshots merge.
pub fn substitute_tree_expr(
    code: impl std::borrow::Borrow<ExprCode>,
    reference: &str,
    replacement: impl std::borrow::Borrow<ExprCode>,
) -> ExprCode {
    let code = code.borrow();
    let replacement = replacement.borrow();
    if !code.free.iter().any(|name| name == reference) {
        return code.clone();
    }
    let tree = super::code_tree::substitute_tree(
        &code.tree,
        reference,
        &replacement.tree,
    );
    let free = super::code_tree::free_names_tree(&tree);
    let mut deps = code.deps.clone();
    deps.extend(replacement.deps.clone());
    ExprCode {
        free,
        tree,
        make: None,
        deps,
    }
}

/// The opened term of a plain Code value (the VM's `open_fragment`
/// over a Code): the same tree with the compiled factory and the
/// capture-time snapshot dropped - an opened term claims nothing.
pub fn open_code(code: impl std::borrow::Borrow<ExprCode>) -> ExprCode {
    let code = code.borrow();
    open_expr(
        super::code_tree::free_names_tree(&code.tree),
        code.tree.clone(),
        None,
        std::collections::BTreeMap::new(),
    )
}

/// `quote.bind` over a code value (the call form; the VM's
/// `mint_binds` + `dependency_snapshot`): nested binder syntax mints
/// fresh tokens and the dependencies re-stamp. The distilled subset
/// carries no binder nodes, so the mint walk is the identity there -
/// the tree and the compiled factory are preserved, and the snapshot
/// re-derives exactly (a stale snapshot heals; a true one is
/// unchanged). The binder FORM of quote.bind (a fresh-tokened
/// function literal) is outside the distilled subset; the lowering
/// refuses it by name and this surface makes no claim about it.
pub fn bind_code(
    code: impl std::borrow::Borrow<ExprCode>,
    module: &super::code_tree::ModuleTable,
) -> ExprCode {
    let code = code.borrow();
    open_expr(
        code.free.clone(),
        code.tree.clone(),
        code.make.clone(),
        super::code_tree::dependency_snapshot_tree(&code.tree, module),
    )
}

/// The guarded executor: stamped dependencies verify against the
/// module table first (the `stale_dependency` refusal), open code
/// refuses `unbound_code` naming every remaining free name in binding
/// order, and closed code computes through the compiled factory when
/// one exists - the made-tree lane evaluates the distilled tree over
/// the shared scalar kernels instead (same carrier rules, same
/// refusal names).
pub fn evaluate_expr(
    code: impl std::borrow::Borrow<ExprCode>,
    module: &super::code_tree::ModuleTable,
) -> Result<CodeValue, String> {
    let code = code.borrow();
    guard_closed(code, module)?;
    if let Some(make) = &code.make {
        return make(&[]);
    }
    super::code_tree::evaluate_tree(&code.tree, &std::collections::BTreeMap::new(), module)
}

/// The executor's guards, in the VM's order: stamped dependencies
/// verify against the module table, then open code refuses
/// `unbound_code` naming every remaining open name.
fn guard_closed(code: &ExprCode, module: &super::code_tree::ModuleTable) -> Result<(), String> {
    super::code_tree::verify_deps(&code.deps, module)?;
    if !code.free.is_empty() {
        return Err(format!(
            "unbound_code: quoted code is open; unbound name(s): {}",
            code.free.join(", ")
        ));
    }
    Ok(())
}

/// A quoted function literal evaluated for application: one call takes
/// the applied arguments in order.
pub type AppliedCode = std::rc::Rc<dyn Fn(&[CodeValue]) -> Result<CodeValue, String>>;

/// The applied lane of the guarded executor (`f = quote.evaluate(code)`
/// then `f(a, ..)`). The code closes exactly as `evaluate_expr` checks
/// it - same refusals, at the same point. The callable applies the
/// literal's nested binders stage by stage (the VM's curried
/// application): each argument binds the next parameter and the body
/// evaluates through the shared scalar tree evaluator. Each argument
/// is checked against its binder's recorded domain BEFORE it binds
/// (the VM's check-then-bind order), refusing `type: closure domain
/// `{d}` does not admit the argument `{v}`` - an `Int` literal never
/// silently computes over a ratio. A closed code that is not a
/// function literal evaluates here (its own refusals first), and every
/// application of it refuses with the VM's
/// `type: value is not callable: {value}`.
pub fn evaluate_callable_expr(
    code: impl std::borrow::Borrow<ExprCode>,
    module: &super::code_tree::ModuleTable,
) -> Result<AppliedCode, String> {
    use super::code_tree::{evaluate_tree, CodeTree};
    let code = code.borrow();
    guard_closed(code, module)?;
    let tree = code.tree.clone();
    let module = *module;
    if !matches!(tree, CodeTree::Closure { .. }) {
        let value = match &code.make {
            Some(make) => make(&[])?,
            None => evaluate_tree(&tree, &std::collections::BTreeMap::new(), &module)?,
        };
        return Ok(std::rc::Rc::new(move |_: &[CodeValue]| {
            Err(format!("type: value is not callable: {value}"))
        }));
    }
    Ok(std::rc::Rc::new(move |args: &[CodeValue]| {
        let mut bound = std::collections::BTreeMap::new();
        let mut current = &tree;
        for argument in args {
            match current {
                CodeTree::Closure {
                    param,
                    domain,
                    body,
                } => {
                    if !domain.admits(argument) {
                        return Err(format!(
                            "type: closure domain `{domain}` does not admit the argument `{argument}`"
                        ));
                    }
                    bound.insert(param.clone(), *argument);
                    current = body;
                }
                other => {
                    let value = evaluate_tree(other, &bound, &module)?;
                    return Err(format!("type: value is not callable: {value}"));
                }
            }
        }
        evaluate_tree(current, &bound, &module)
    }))
}

/// The free names in binding order.
pub fn free_names_expr(code: &ExprCode) -> &[String] {
    &code.free
}

impl ExprCode {
    /// The artifact-side walk surface (method syntax: emitted
    /// registers hold this value owned, borrowed, or inlined, and a
    /// read must never move a multi-use register). Each method is
    /// the same law as its free function.
    pub fn view(&self, module: &super::code_tree::ModuleTable) -> super::code_tree::NodeValue {
        super::code_tree::view_quoted(self, module)
    }
    pub fn identity(&self) -> Result<i64, String> {
        super::code_tree::tree_identity(&self.tree)
    }
    pub fn substitute(&self, reference: &str, value: CodeValue) -> ExprCode {
        substitute_expr(self, reference, value)
    }
    /// The structural substitution (a made subtree as the
    /// replacement - the VM's `value_to_expr` law).
    pub fn substitute_tree(
        &self,
        reference: &str,
        replacement: impl std::borrow::Borrow<ExprCode>,
    ) -> ExprCode {
        substitute_tree_expr(self, reference, replacement)
    }
    pub fn evaluate(&self, module: &super::code_tree::ModuleTable) -> Result<CodeValue, String> {
        evaluate_expr(self, module)
    }
    pub fn evaluate_callable(
        &self,
        module: &super::code_tree::ModuleTable,
    ) -> Result<AppliedCode, String> {
        evaluate_callable_expr(self, module)
    }
    /// The definition-table unfold (`quote.body`): the Available/
    /// Opaque record over the embedded definition table (the VM's
    /// `quote_body` laws; a transparent body outside the distilled
    /// subset refuses by name).
    pub fn body(
        &self,
        defs: &super::code_tree::DefinitionTable,
    ) -> Result<super::code_tree::NodeValue, String> {
        super::code_tree::quote_body_node(self, defs)
    }
    /// `quote.open` over a plain Code value: the opened term, factory
    /// and snapshot dropped (the VM's `open_fragment` law).
    pub fn open(&self) -> ExprCode {
        open_code(self)
    }
    /// `quote.bind` over a Code value (the call form): the mint walk
    /// (identity over the distilled subset) with the dependency
    /// snapshot re-stamped.
    pub fn bind(&self, module: &super::code_tree::ModuleTable) -> ExprCode {
        bind_code(self, module)
    }
}

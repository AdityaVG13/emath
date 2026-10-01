//! Lower constructor-layer functions onto generic EMIR.
//!
//! Scalar operators stay representation ops. Executable quote transforms
//! are eliminated to residual scalar expressions before lowering. Opaque
//! or missing transformation rules stay unresolved and are not marked
//! runnable.

use emath_core::Span;
use emath_core::tree::{
    BinaryOp, Declaration, Expr, ExprKind, GenericArg, Item, StmtKind, SyntaxTree, TypeExpr,
    TypeKind, UnaryOp,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::constructor_layer::{machine_buffer_basename, machine_int_basename};
use crate::exact_int::ExactInt;
use crate::{CodeApplication, CodeSubRef, DomainObligation, EmirOp, EmirProgram, EmirValue};

#[derive(Clone, Debug, PartialEq)]
pub struct LoweredFunction {
    pub program: EmirProgram,
    pub inputs: Vec<String>,
    pub runnable: bool,
    pub unresolved: Vec<String>,
}

pub fn lower_constructor_function(
    tree: &SyntaxTree,
    name: &str,
) -> Result<LoweredFunction, String> {
    ConstructorLowering::new(tree).lower(name)
}

/// One module's lowering session. Every root lowers exactly as
/// [`lower_constructor_function`] would, but roots lowered through one
/// session share each PATH-INDEPENDENT callee result: a runnable
/// function whose lowering met no open ancestor (no cycle edge, no
/// `CallSibling` anywhere in its inlined tree) and whose every inlined
/// callee is itself path-independent. Such a result cannot depend on
/// which root reached it - an ancestor reachable from the function
/// would close a cycle through it and surface as a cycle edge in its
/// own lowering - so sharing it changes no emitted byte and spares the
/// crate emission from re-lowering every callee tree once per root.
pub struct ConstructorLowering<'a> {
    tree: &'a SyntaxTree,
    shared: BTreeMap<String, LoweredFunction>,
}

impl<'a> ConstructorLowering<'a> {
    pub fn new(tree: &'a SyntaxTree) -> Self {
        Self {
            tree,
            shared: BTreeMap::new(),
        }
    }

    /// Lower one root function (see [`lower_constructor_function`]).
    ///
    /// # Errors
    /// The same named lowering refusals as [`lower_constructor_function`].
    pub fn lower(&mut self, name: &str) -> Result<LoweredFunction, String> {
        let mut cache = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        lower_named(self.tree, name, &mut cache, &mut visiting, &mut self.shared)
    }
}

/// `f = quote.evaluate(cand)`: the def binds the specialized unary
/// closure, so later `f(x)` calls lower as closure calls (`CallValue`),
/// not field access.
fn is_quote_evaluate_call(function: &Expr) -> bool {
    let ExprKind::Path { segments, .. } = &function.kind else {
        return false;
    };
    segments.len() == 2 && segments[0] == "quote" && segments[1] == "evaluate"
}

/// A def whose RHS is a call to a declared function with exactly one
/// arrow output binds a closure value (`fam = MakeFamily(0)`, the
/// session-surface lift pattern): the callee's declaration is the
/// type authority, the same source the input carriers use.
fn call_binds_closure(tree: &SyntaxTree, function: &Expr) -> bool {
    let ExprKind::Path { segments, .. } = &function.kind else {
        return false;
    };
    if segments.len() != 1 {
        return false;
    }
    tree.items.iter().any(|item| match item {
        Item::Declaration(decl) if decl.as_kind == "function" && decl.name == segments[0] => {
            let outputs = section_typed_fields(decl, "outputs");
            outputs.len() == 1
                && outputs
                    .iter()
                    .all(|(_, ty)| matches!(ty.kind, TypeKind::Fn { .. }))
        }
        _ => false,
    })
}

/// `h = blk.apply` (`Path[blk, apply]`, or `Call { Path[apply], [blk] }`)
/// where `blk` is a declared input of an object type whose `apply`
/// field is declared as an arrow: the def binds that closure, and its
/// declared stage count lets `h(x, y)` apply curried. The object's
/// representation is the type authority. `base` is the path whose
/// head names the input.
fn field_arrow_stages(
    tree: &SyntaxTree,
    decl: &Declaration,
    base: &Expr,
    field: &str,
) -> Option<usize> {
    let ExprKind::Path { segments, .. } = &base.kind else {
        return None;
    };
    let base = segments.first()?;
    let (_, ty) = section_typed_fields(decl, "inputs")
        .into_iter()
        .find(|(input, _)| input == base)?;
    let TypeKind::Path { segments, .. } = &ty.kind else {
        return None;
    };
    let [object] = segments.as_slice() else {
        return None;
    };
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(object_decl)
            if object_decl.as_kind == "object" && &object_decl.name == object =>
        {
            section_typed_fields(object_decl, "representation")
                .into_iter()
                .find(|(name, _)| name == field)
                .map(|(_, ty)| arrow_type_stages(&ty))
                .filter(|stages| *stages > 0)
        }
        _ => None,
    })
}

/// Authored carrier signatures for one function: input carriers (one
/// per declared input) and the output carrier (a multi-output
/// function packs `Record<name>`, matching the entry emission's
/// result authority). Empty strings mean unknown.
fn authored_signatures(
    tree: &SyntaxTree,
    name: &str,
    object_names: &BTreeSet<String>,
) -> (Vec<String>, String) {
    let decl = tree.items.iter().find_map(|item| match item {
        Item::Declaration(decl) if decl.as_kind == "function" && decl.name == name => Some(decl),
        _ => None,
    });
    let Some(decl) = decl else {
        return (Vec::new(), String::new());
    };
    let vars: Vec<String> = decl
        .generics
        .iter()
        .map(|generic| generic.name.clone())
        .collect();
    let declared = section_typed_fields(decl, "inputs")
        .iter()
        .map(|(_, ty)| type_signature(ty, object_names, &vars).unwrap_or_default())
        .collect();
    let outputs = section_typed_fields(decl, "outputs");
    let result = if outputs.len() > 1 {
        format!("Record<{name}>")
    } else {
        outputs
            .first()
            .and_then(|(_, ty)| constructor_type_signature(ty, object_names))
            .unwrap_or_default()
    };
    (declared, result)
}

/// A generic single-output function's result signature with its type
/// parameters spelled `Var<name>` (`repeat<T>` yields `Vector<Var<T>>`);
/// `None` for a non-generic or multi-output function.
fn generic_result_signature(
    tree: &SyntaxTree,
    name: &str,
    object_names: &BTreeSet<String>,
) -> Option<String> {
    let decl = tree.items.iter().find_map(|item| match item {
        Item::Declaration(decl) if decl.as_kind == "function" && decl.name == name => Some(decl),
        _ => None,
    })?;
    if decl.generics.is_empty() {
        return None;
    }
    let vars: Vec<String> = decl
        .generics
        .iter()
        .map(|generic| generic.name.clone())
        .collect();
    let [(_, ty)] = section_typed_fields(decl, "outputs").try_into().ok()?;
    type_signature(&ty, object_names, &vars)
}

fn lower_named(
    tree: &SyntaxTree,
    name: &str,
    cache: &mut BTreeMap<String, LoweredFunction>,
    visiting: &mut BTreeSet<String>,
    shared: &mut BTreeMap<String, LoweredFunction>,
) -> Result<LoweredFunction, String> {
    if let Some(hit) = shared.get(name) {
        return Ok(hit.clone());
    }
    if let Some(hit) = cache.get(name) {
        return Ok(hit.clone());
    }
    if !visiting.insert(name.to_string()) {
        return Err(format!("recursive sibling `{name}` is not emitted"));
    }
    let decl = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(decl) if decl.as_kind == "function" && decl.name == name => {
                Some(decl)
            }
            _ => None,
        })
        .ok_or_else(|| format!("unknown function `{name}`"))?;
    let inputs = section_fields(decl, "inputs");
    let outputs = section_fields(decl, "outputs");
    let authored_defs = section_assigns(decl, "definitions");
    let defs = if authored_defs.iter().any(|(_, expr)| expr_uses_quote(expr)) {
        match crate::constructor_layer::residual_output_exprs(tree, name) {
            Ok(residual) => residual,
            Err(_) => authored_defs,
        }
    } else {
        authored_defs
    };
    let mut unresolved = Vec::new();
    for (_, expr) in &defs {
        collect_unresolved(expr, &mut unresolved);
    }
    let mut siblings = BTreeMap::new();
    let mut entry_refs = BTreeMap::new();
    let mut call_sites = BTreeMap::new();
    for (_, expr) in &defs {
        collect_call_sites(expr, &mut call_sites);
    }
    let mut callees: BTreeSet<String> = call_sites.keys().cloned().collect();
    // A module function named as a value (`map(double, xs)`,
    // `{apply: float_tanh}`) lowers too: the value is a closure that
    // calls it. Inputs and definitions shadow function names.
    let shadowing: BTreeSet<&String> = inputs
        .iter()
        .chain(defs.iter().map(|(def, _)| def))
        .collect();
    for (_, expr) in &defs {
        let mut free = BTreeSet::new();
        collect_free_names(expr, "", &mut free);
        callees.extend(
            free.into_iter()
                .filter(|free| !shadowing.contains(free) && function_exists(tree, free)),
        );
    }
    let objects = object_kinds(tree);
    let object_names: BTreeSet<String> = objects.keys().cloned().collect();
    // Path independence (see `ConstructorLowering`): no open ancestor
    // met here, and every inlined callee path-independent itself.
    let mut independent = true;
    for callee in callees {
        if callee == name || !function_exists(tree, &callee) {
            continue;
        }
        if visiting.contains(&callee) {
            independent = false;
            // The cycle edge: this callee is an open ancestor, so its
            // body cannot inline here (the cycle would nest forever).
            // Reference its emitted entry fn instead - the backend
            // renders the named call, crossing arguments and results
            // through the callee's declared carriers.
            let signatures = authored_signatures(tree, &callee, &object_names);
            entry_refs.insert(callee, signatures);
            continue;
        }
        match lower_named(tree, &callee, cache, visiting, shared) {
            Ok(other) if other.runnable => {
                independent &= shared.contains_key(&callee);
                // Authored input carriers for the inlined frame: an
                // argument with no self-evident kind (an empty `[]`)
                // recovers its carrier from the callee's declaration.
                let (declared, _) = authored_signatures(tree, &callee, &object_names);
                siblings.insert(callee, (other.program, declared));
            }
            Ok(other) => {
                unresolved.extend(other.unresolved);
                unresolved.push(callee);
            }
            Err(err) => unresolved.push(err),
        }
    }
    unresolved.sort();
    unresolved.dedup();
    let hints = TypeHints {
        object_fields: tree
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Declaration(object) if object.as_kind == "object" => Some(object),
                _ => None,
            })
            .filter(|object| object.generics.is_empty())
            .map(|object| {
                let fields = section_typed_fields(object, "representation")
                    .into_iter()
                    .filter_map(|(field, ty)| {
                        constructor_type_signature(&ty, &object_names).map(|sig| (field, sig))
                    })
                    .collect();
                (object.name.clone(), fields)
            })
            .collect(),
        sibling_results: siblings
            .keys()
            .filter_map(|callee| {
                generic_result_signature(tree, callee, &object_names)
                    .map(|sig| (callee.clone(), sig))
            })
            .collect(),
    };
    let mut lowerer = Lowerer {
        inputs: inputs.clone(),
        locals: BTreeMap::new(),
        siblings: std::rc::Rc::new(siblings),
        entry_siblings: entry_refs,
        objects,
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name: Some(name.to_string()),
        // The authored output grounds self-recursion kind inference:
        // multi-output functions pack a `Record<name>` result.
        self_result: {
            let outputs = section_typed_fields(decl, "outputs");
            if outputs.len() > 1 {
                format!("Record<{name}>")
            } else {
                outputs
                    .first()
                    .and_then(|(_, ty)| constructor_type_signature(ty, &object_names))
                    .unwrap_or_default()
            }
        },
        arrow_names: section_typed_fields(decl, "inputs")
            .iter()
            .filter(|(_, ty)| matches!(ty.kind, TypeKind::Fn { .. }))
            .map(|(input, _)| input.clone())
            .collect(),
        // The module callable stamps: quote captures stamp free names
        // against this table (the shared-tree lane's dependency law).
        module_stamps: crate::constructor_layer::module_callable_table(tree)
            .into_iter()
            .map(|(callable, _, stamp)| (callable, stamp))
            .collect(),
        type_vars: decl
            .generics
            .iter()
            .map(|generic| generic.name.clone())
            .collect(),
        hints: std::rc::Rc::new(hints),
    };
    if !unresolved.is_empty() {
        visiting.remove(name);
        let result = lowerer.push(EmirOp::Refuse(format!(
            "unresolved symbolic code: {}",
            unresolved.join(", ")
        )));
        let lowered = LoweredFunction {
            program: lowerer.finish(result),
            inputs,
            runnable: false,
            unresolved,
        };
        cache.insert(name.to_string(), lowered.clone());
        return Ok(lowered);
    }
    // Known curried stage counts by name (declared arrow inputs,
    // function-literal defs, and defs that bind a partial application
    // of either): an application that leaves stages unapplied binds a
    // closure value.
    let mut arrow_stages: BTreeMap<String, usize> = section_typed_fields(decl, "inputs")
        .into_iter()
        .filter_map(|(input, ty)| {
            let stages = arrow_type_stages(&ty);
            (stages > 0).then_some((input, stages))
        })
        .collect();
    let output_signature = authored_signatures(tree, name, &object_names).1;
    for (def_name, expr) in &defs {
        // `f = quote.evaluate(code)` used as a function: passed into a
        // declared arrow parameter of a module function (one agreed
        // signature - `newton_solve(f, df, ..)` with `df: Rat -> Rat`),
        // or applied in place with one agreed argument count. The
        // evaluated value is then a quoted function literal used as a
        // callable (the VM's law - evaluate yields the closure).
        let applied = match &expr.kind {
            ExprKind::Call { function, args }
                if is_quote_evaluate_call(function) && args.len() == 1 =>
            {
                evaluate_application(tree, &call_sites, def_name, &object_names)
            }
            _ => None,
        };
        let lowered = match (&expr.kind, applied) {
            (ExprKind::Call { args, .. }, Some(applied)) => {
                lowerer.lower_applied_evaluate(&args[0], applied)
            }
            // The single output's declared carrier is the expectation
            // for a generic call handed evaluated code
            // (`tabulate(quote.evaluate(c), n)` into `sequence(Int)`
            // binds `T`, so the evaluated code is an `Int -> Int`).
            (ExprKind::Call { function, args }, None)
                if outputs.len() == 1
                    && &outputs[0] == def_name
                    && !output_signature.is_empty()
                    && matches!(&function.kind, ExprKind::Path { segments, .. }
                        if segments.len() == 1 && is_generic_function(tree, &segments[0]))
                    && args.iter().any(|arg| {
                        matches!(&arg.kind,
                        ExprKind::Call { function, .. } if is_quote_evaluate_call(function))
                    }) =>
            {
                lowerer.expr_expecting(expr, &output_signature)
            }
            _ => lowerer.expr(expr),
        };
        match lowered {
            Ok(value) => {
                lowerer.locals.insert(def_name.clone(), value);
                // A def bound to a closure value is closure-valued:
                // later one-argument calls on this name are closure
                // calls, not field access. Sources: a function
                // literal, a path naming a known arrow name, a call
                // to a declared function with a single arrow output
                // (`fam = MakeFamily(0)` then `fam(1/2)` is the lift
                // pattern's applied-local shape), a
                // `quote.evaluate` call (`f = quote.evaluate(cand)`
                // then `f(x)` is the program-space application seam),
                // a path naming a module function (`h = weighted`
                // coerces curried, one stage per input), an
                // arrow-declared object field (`h = blk.apply`),
                // and a partial application of a declared arrow
                // (`model_at = predict(params)` on `A -> B -> C` binds
                // the `B -> C` stage - the curried model seam).
                let residual = match &expr.kind {
                    ExprKind::FunctionAbs { .. } => Some(literal_stages(expr)),
                    ExprKind::Path { segments, .. } if segments.len() == 2 => {
                        field_arrow_stages(tree, decl, expr, &segments[1])
                    }
                    ExprKind::Path { segments, .. } if segments.len() == 1 => {
                        let head = &segments[0];
                        arrow_stages.get(head).copied().or_else(|| {
                            let shadowed =
                                lowerer.locals.contains_key(head) || lowerer.inputs.contains(head);
                            lowerer
                                .siblings
                                .get(head)
                                .filter(|_| !shadowed)
                                .map(|(_, declared)| declared.len())
                                .filter(|stages| *stages > 0)
                        })
                    }
                    ExprKind::Call { function, args } => match &function.kind {
                        ExprKind::Path { segments, .. } if segments.len() == 1 => arrow_stages
                            .get(&segments[0])
                            .and_then(|stages| stages.checked_sub(args.len()))
                            .filter(|remaining| *remaining > 0)
                            .or_else(|| match args.as_slice() {
                                [base] => field_arrow_stages(tree, decl, base, &segments[0]),
                                _ => None,
                            }),
                        _ => None,
                    },
                    _ => None,
                };
                let closure_valued = residual.is_some()
                    || match &expr.kind {
                        ExprKind::FunctionAbs { .. } => true,
                        ExprKind::Path { segments, .. } => {
                            segments.len() == 1 && lowerer.arrow_names.contains(&segments[0])
                        }
                        ExprKind::Call { function, .. } => {
                            call_binds_closure(tree, function) || is_quote_evaluate_call(function)
                        }
                        _ => false,
                    };
                if let Some(residual) = residual {
                    arrow_stages.insert(def_name.clone(), residual);
                }
                if closure_valued {
                    lowerer.arrow_names.insert(def_name.clone());
                }
            }
            Err(err) => {
                visiting.remove(name);
                unresolved.push(err);
                let result = lowerer.push(EmirOp::Refuse(format!(
                    "unresolved symbolic code: {}",
                    unresolved.join(", ")
                )));
                let lowered = LoweredFunction {
                    program: lowerer.finish(result),
                    inputs,
                    runnable: false,
                    unresolved,
                };
                cache.insert(name.to_string(), lowered.clone());
                return Ok(lowered);
            }
        }
    }
    let result = if outputs.len() > 1 {
        let mut fields = Vec::new();
        for output in &outputs {
            let Some(value) = lowerer.locals.get(output).copied() else {
                visiting.remove(name);
                return Err(format!("output `{output}` has no definition"));
            };
            fields.push((output.clone(), value));
        }
        lowerer.push(EmirOp::RecordCreate {
            type_name: name.to_string(),
            fields,
        })
    } else {
        let output = outputs
            .first()
            .cloned()
            .or_else(|| defs.last().map(|(def_name, _)| def_name.clone()))
            .ok_or_else(|| format!("function `{name}` has no output"))?;
        lowerer
            .locals
            .get(&output)
            .copied()
            .ok_or_else(|| format!("function `{name}` has no definition"))?
    };
    visiting.remove(name);
    let lowered = LoweredFunction {
        program: lowerer.finish(result),
        inputs,
        runnable: true,
        unresolved,
    };
    if independent && !contains_call_sibling(&lowered.program) {
        shared.insert(name.to_string(), lowered.clone());
    }
    cache.insert(name.to_string(), lowered.clone());
    Ok(lowered)
}

/// Whether a program carries a recursion-cycle edge at any depth.
fn contains_call_sibling(program: &EmirProgram) -> bool {
    program.ops.iter().any(|(op, _)| match op {
        EmirOp::CallSibling { .. } => true,
        EmirOp::Branch {
            then_body,
            else_body,
            ..
        } => contains_call_sibling(then_body) || contains_call_sibling(else_body),
        EmirOp::CallFrame { body, .. }
        | EmirOp::ProgramLiteral { body, .. }
        | EmirOp::Fold { body, .. }
        | EmirOp::Collect { body, .. } => contains_call_sibling(body),
        EmirOp::Iterate { body, stop, .. } => {
            contains_call_sibling(body) || stop.as_ref().is_some_and(contains_call_sibling)
        }
        _ => false,
    })
}

/// Curried stage count of a declared type: one argument per stage,
/// `A -> B -> C` folds as `A -> (B -> C)` (two stages); 0 for a
/// non-arrow.
fn arrow_type_stages(ty: &TypeExpr) -> usize {
    match &ty.kind {
        TypeKind::Fn { codomain, .. } => 1 + arrow_type_stages(codomain),
        _ => 0,
    }
}

/// Stage count a function literal is known to carry: one per nested
/// literal (`function p: function t: ..` is two stages). A body that
/// yields a closure some other way is not counted - an undercount only
/// leaves a later application on the field-access reading, which the
/// backend refuses by name over a callable carrier.
fn literal_stages(expr: &Expr) -> usize {
    match &expr.kind {
        ExprKind::FunctionAbs { body, .. } => 1 + literal_stages(body),
        _ => 0,
    }
}

/// How an evaluated def is used as a function, from its call sites:
/// the one declared arrow signature every module-function parameter it
/// is passed into agrees on, else the one argument count it is applied
/// with. Anything else (no use as a function, disagreeing uses) keeps
/// the plain evaluated value.
fn evaluate_application(
    tree: &SyntaxTree,
    call_sites: &BTreeMap<String, Vec<Vec<Option<String>>>>,
    def_name: &str,
    object_names: &BTreeSet<String>,
) -> Option<CodeApplication> {
    let mut declared = BTreeSet::new();
    for (callee, sites) in call_sites {
        if callee == def_name || !function_exists(tree, callee) {
            continue;
        }
        let (inputs, _) = authored_signatures(tree, callee, object_names);
        for site in sites {
            for (position, arg) in site.iter().enumerate() {
                if arg.as_deref() == Some(def_name) {
                    match inputs.get(position) {
                        Some(signature)
                            if signature.starts_with("Fn<") && !signature.contains("Var<") =>
                        {
                            declared.insert(signature.clone());
                        }
                        _ => return None,
                    }
                }
            }
        }
    }
    if declared.len() == 1 {
        return declared.pop_first().map(CodeApplication::Declared);
    }
    if !declared.is_empty() {
        return None;
    }
    let arities: BTreeSet<usize> = call_sites.get(def_name)?.iter().map(Vec::len).collect();
    let [arity] = arities.into_iter().collect::<Vec<_>>()[..] else {
        return None;
    };
    u16::try_from(arity).ok().map(CodeApplication::Union)
}

fn is_generic_function(tree: &SyntaxTree, name: &str) -> bool {
    tree.items.iter().any(|item| {
        matches!(
            item,
            Item::Declaration(decl)
                if decl.as_kind == "function" && decl.name == name && !decl.generics.is_empty()
        )
    })
}

fn function_exists(tree: &SyntaxTree, name: &str) -> bool {
    tree.items.iter().any(|item| {
        matches!(
            item,
            Item::Declaration(decl) if decl.as_kind == "function" && decl.name == name
        )
    })
}

struct Lowerer {
    inputs: Vec<String>,
    locals: BTreeMap<String, EmirValue>,
    /// Inlined sibling programs with their authored input carrier
    /// signatures (one per declared input, empty = unknown). The
    /// backend recovers degenerate argument kinds (an empty `[]`)
    /// from the declaration instead of guessing.
    /// Shared (`Rc`): nested closure lowerers see the same map, and a
    /// map of whole inlined programs is never deep-copied per scope.
    siblings: std::rc::Rc<BTreeMap<String, (EmirProgram, Vec<String>)>>,
    /// Sibling calls that close a recursion cycle (the callee is an
    /// open ancestor, so its body cannot inline): name -> authored
    /// (input carriers, output carrier). Lowered as `CallSibling` -
    /// the backend renders a call to the callee's emitted entry fn.
    entry_siblings: BTreeMap<String, (Vec<String>, String)>,
    objects: BTreeMap<String, String>,
    ops: Vec<(EmirOp, Span)>,
    obligations: Vec<DomainObligation>,
    self_name: Option<String>,
    /// The enclosing function's authored output carrier signature
    /// (empty = unknown). `CallSelf` sites carry it so backend kind
    /// inference reads the declared result instead of guessing from
    /// the first argument.
    self_result: String,
    /// Names known to be closure-valued: arrow-declared inputs, defs
    /// bound to function literals, arrow-domain params of nested
    /// literals. A one-argument call on any OTHER name is field access
    /// (`base.field` parses as `Call { Path[field], [base] }`), so
    /// this set is what disambiguates closure calls from projections.
    arrow_names: BTreeSet<String>,
    /// The module callable table's stamps (name -> declaration
    /// identity): the quote capture stamps free names that resolve
    /// against it, exactly as the VM's `dependency_snapshot` does.
    module_stamps: BTreeMap<String, u64>,
    /// The enclosing declaration's type parameters: a binder domain
    /// naming one lowers as `Var<name>`, which the backend binds per
    /// inlined call site.
    type_vars: Vec<String>,
    /// Declared carriers a call site can be checked against from the
    /// outside (see [`TypeHints`]).
    hints: std::rc::Rc<TypeHints>,
}

/// Carriers known from where a value lands rather than from the value
/// itself: an authored object's field signatures, and each inlined
/// generic sibling's result signature with its type parameters
/// (`Vector<Var<T>>`). `repeat(repeat([], 8), 8)` written into a
/// `sequence(sequence(sequence(Rule)))` field has no element carrier
/// of its own; the field binds `T` for both calls.
#[derive(Default)]
struct TypeHints {
    object_fields: BTreeMap<String, BTreeMap<String, String>>,
    sibling_results: BTreeMap<String, String>,
}

/// Binds the `Var<name>`s of `pattern` against the concrete carrier
/// signature `concrete`, structurally (`Vector<Var<T>>` against
/// `Vector<Record<Rule>>` binds `T` to `Record<Rule>`). Mismatched
/// shapes bind nothing; the first binding of a name wins.
fn bind_signature_vars(pattern: &str, concrete: &str, bound: &mut BTreeMap<String, String>) {
    if let Some(name) = pattern
        .strip_prefix("Var<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        bound
            .entry(name.to_string())
            .or_insert_with(|| concrete.to_string());
        return;
    }
    let (Some((head, args)), Some((concrete_head, concrete_args))) = (
        pattern
            .strip_suffix('>')
            .and_then(|text| text.split_once('<')),
        concrete
            .strip_suffix('>')
            .and_then(|text| text.split_once('<')),
    ) else {
        return;
    };
    let (args, concrete_args) = (signature_args(args), signature_args(concrete_args));
    if head == concrete_head && args.len() == concrete_args.len() {
        for (arg, concrete_arg) in args.iter().zip(&concrete_args) {
            bind_signature_vars(arg, concrete_arg, bound);
        }
    }
}

/// The top-level comma-separated arguments of a signature's `<...>`.
fn signature_args(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (index, ch) in text.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

impl Lowerer {
    fn push(&mut self, op: EmirOp) -> EmirValue {
        let index = u32::try_from(self.ops.len()).expect("emir op count");
        self.ops.push((op, Span::default()));
        EmirValue(index)
    }

    fn current_inputs(&mut self) -> Vec<EmirValue> {
        (0..self.inputs.len())
            .map(|index| self.push(EmirOp::LoadInput(index as u16)))
            .collect()
    }

    /// Lowers `expr` where its value must have the carrier `expected`.
    /// A call to an inlined generic sibling binds its type parameters
    /// from `expected` against the callee's result, and the inlined
    /// frame declares the instantiated input carriers, so an argument
    /// with no carrier of its own (`[]`) gets one; the arguments are
    /// lowered against those carriers in turn. Anything else lowers
    /// as usual.
    fn expr_expecting(&mut self, expr: &Expr, expected: &str) -> Result<EmirValue, String> {
        let ExprKind::Call { function, args } = &expr.kind else {
            return self.expr(expr);
        };
        // Evaluated code expected as a function takes the declared
        // arrow's callable lane.
        if is_quote_evaluate_call(function)
            && args.len() == 1
            && expected.starts_with("Fn<")
            && !expected.contains("Var<")
        {
            return self
                .lower_applied_evaluate(&args[0], CodeApplication::Declared(expected.to_string()));
        }
        let ExprKind::Path { segments, .. } = &function.kind else {
            return self.expr(expr);
        };
        let [called] = segments.as_slice() else {
            return self.expr(expr);
        };
        let inlined = self.self_name.as_deref() != Some(called.as_str())
            && !self.entry_siblings.contains_key(called)
            && !self.locals.contains_key(called)
            && !self.inputs.contains(called);
        let (Some((body, declared)), Some(result)) = (
            self.siblings.get(called).filter(|_| inlined).cloned(),
            self.hints.sibling_results.get(called).cloned(),
        ) else {
            return self.expr(expr);
        };
        if declared.len() != args.len() {
            return self.expr(expr);
        }
        let mut bound = BTreeMap::new();
        bind_signature_vars(&result, expected, &mut bound);
        let declared: Vec<String> = declared
            .iter()
            .map(|signature| {
                bound
                    .iter()
                    .fold(signature.clone(), |text, (var, carrier)| {
                        text.replace(&format!("Var<{var}>"), carrier)
                    })
            })
            .collect();
        let mut inputs = Vec::with_capacity(args.len());
        for (arg, signature) in args.iter().zip(&declared) {
            inputs.push(if signature.is_empty() || signature.contains("Var<") {
                self.expr(arg)?
            } else {
                self.expr_expecting(arg, signature)?
            });
        }
        Ok(self.push(EmirOp::CallFrame {
            body,
            inputs,
            state: Vec::new(),
            declared,
            callee: Some(called.clone()),
        }))
    }

    fn nested(&self, expr: &Expr) -> Result<EmirProgram, String> {
        let mut lowerer = self.capture_child();
        let result = lowerer.expr(expr)?;
        Ok(lowerer.finish(result))
    }

    fn captured_names(&self) -> Vec<String> {
        let mut names = self.inputs.clone();
        names.extend(self.locals.keys().cloned());
        names
    }

    fn captured_args(&mut self) -> Vec<EmirValue> {
        let mut args = self.current_inputs();
        for value in self.locals.values() {
            args.push(*value);
        }
        args
    }

    fn capture_child(&self) -> Lowerer {
        Lowerer {
            inputs: self.captured_names(),
            locals: BTreeMap::new(),
            siblings: self.siblings.clone(),
            entry_siblings: self.entry_siblings.clone(),
            objects: self.objects.clone(),
            ops: Vec::new(),
            obligations: Vec::new(),
            self_name: self.self_name.clone(),
            self_result: self.self_result.clone(),
            arrow_names: self.arrow_names.clone(),
            module_stamps: self.module_stamps.clone(),
            type_vars: self.type_vars.clone(),
            hints: self.hints.clone(),
        }
    }

    fn lower_recur_apply(
        &mut self,
        name: &str,
        body: &Expr,
        args: &[Expr],
    ) -> Result<EmirValue, String> {
        let (param, inner) = match &body.kind {
            ExprKind::FunctionAbs { param, body, .. } => (param.as_str(), body.as_ref()),
            _ => return Err("recur emission expects a function literal".into()),
        };
        let program = lower_int_primitive_recur(name, param, inner)
            .map_or_else(|| lower_general_recur(name, param, inner), Ok)?;
        let mut inputs = Vec::new();
        for arg in args {
            inputs.push(self.expr(arg)?);
        }
        Ok(self.push(EmirOp::CallFrame {
            body: program,
            inputs,
            state: Vec::new(),
            declared: Vec::new(),
            callee: None,
        }))
    }

    /// Lower a function literal capture-aware: free names of the body
    /// that resolve in the enclosing frame become explicit capture
    /// inputs, so a nested literal like `function k in Int: function cs
    /// in CS: k / 8` closes over `k` instead of faulting unbound. The
    /// returned capture values are enclosing-frame registers; both the
    /// value form (`ProgramLiteral`) and immediate application (`CallFrame`)
    /// place them after the explicit arguments. The child inherits the
    /// arrow-ness of captured closure names; an arrow-domain parameter
    /// is itself callable.
    fn lower_function_abs(
        &mut self,
        param: &str,
        domain: &Expr,
        body: &Expr,
    ) -> Result<(EmirProgram, Vec<EmirValue>), String> {
        let mut free = BTreeSet::new();
        collect_free_names(body, param, &mut free);
        let mut captures: Vec<String> = Vec::new();
        for name in &free {
            if self.inputs.contains(name) || self.locals.contains_key(name) {
                captures.push(name.clone());
            }
        }
        let mut inputs = vec![param.to_string()];
        inputs.extend(captures.iter().cloned());
        let mut arrow_names = self
            .arrow_names
            .iter()
            .filter(|name| inputs.contains(*name))
            .cloned()
            .collect::<BTreeSet<_>>();
        if is_arrow_domain(domain) {
            arrow_names.insert(param.to_string());
        }
        // A literal's call to the enclosing function (the structural
        // walks' `any(function k in Code: walk(k), kids)`) is that
        // function's recursion, unless the literal binds the name.
        let self_name = self.self_name.clone().filter(|name| !inputs.contains(name));
        let mut child = Lowerer {
            inputs,
            locals: BTreeMap::new(),
            siblings: self.siblings.clone(),
            entry_siblings: self.entry_siblings.clone(),
            objects: self.objects.clone(),
            ops: Vec::new(),
            obligations: Vec::new(),
            self_result: if self_name.is_some() {
                self.self_result.clone()
            } else {
                String::new()
            },
            self_name,
            arrow_names,
            module_stamps: self.module_stamps.clone(),
            type_vars: self.type_vars.clone(),
            hints: self.hints.clone(),
        };
        let result = child.expr(body)?;
        let program = child.finish(result);
        let mut values = Vec::with_capacity(captures.len());
        for name in &captures {
            if let Some(index) = self.inputs.iter().position(|input| input == name) {
                values.push(self.push(EmirOp::LoadInput(index as u16)));
            } else if let Some(value) = self.locals.get(name) {
                values.push(*value);
            }
        }
        Ok((program, values))
    }

    fn lower_cases(&mut self, arms: &[(Expr, Expr)], else_arm: &Expr) -> Result<EmirValue, String> {
        let Some((condition, then_value)) = arms.first() else {
            return self.expr(else_arm);
        };
        let condition = self.expr(condition)?;
        let then_body = self.nested(then_value)?;
        let else_body = {
            let mut inner = self.capture_child();
            let result = inner.lower_cases(&arms[1..], else_arm)?;
            inner.finish(result)
        };
        let args = self.captured_args();
        Ok(self.push(EmirOp::Branch {
            condition,
            args,
            then_body,
            else_body,
        }))
    }

    fn lookup_path(&mut self, segments: &[String]) -> Result<EmirValue, String> {
        let name = segments.join(".");
        if let Some(value) = self.locals.get(&name) {
            return Ok(*value);
        }
        if let Some(index) = self.inputs.iter().position(|input| input == &name) {
            return Ok(self.push(EmirOp::LoadInput(index as u16)));
        }
        if segments.len() >= 2 {
            // Nested record projection: resolve the head, then chain one
            // RecordField per remaining hop (`o.inner.x` projects twice).
            let record = self.lookup_path(&segments[..segments.len() - 1])?;
            return Ok(self.push(EmirOp::RecordField {
                record,
                field: segments[segments.len() - 1].clone(),
            }));
        }
        // A bare schema-tag name (`Call`, `Add`, `transparent`) is a
        // node-family tag VALUE - the same resolution the VM's path
        // eval performs (`is_schema_tag` -> `schema_tag`). It lowers
        // as a node record with no fields, so the structural walk's
        // `node.kind == Call` comparisons admit identically in both
        // lanes.
        if segments.len() == 1 && crate::constructor_layer::is_node_tag(&name) {
            return Ok(self.push(EmirOp::RecordCreate {
                type_name: name,
                fields: Vec::new(),
            }));
        }
        let siblings = std::rc::Rc::clone(&self.siblings);
        if segments.len() == 1
            && let Some((body, declared)) = siblings.get(&name)
        {
            return self.named_function_value(&name, body, declared);
        }
        Err(format!("unbound `{name}` in emission"))
    }

    /// A module function named as a value: the curried closure literal
    /// `function a in A: function b in B: name(a, b)` over its declared
    /// input carriers (the VM's coercion of a named function).
    fn named_function_value(
        &mut self,
        name: &str,
        body: &EmirProgram,
        declared: &[String],
    ) -> Result<EmirValue, String> {
        if declared.is_empty() {
            return Err(format!(
                "function `{name}` used as a value must take at least one input"
            ));
        }
        if declared
            .iter()
            .any(|signature| signature.is_empty() || signature.contains("Var<"))
        {
            return Err(format!(
                "function `{name}` used as a value needs concrete declared input carriers"
            ));
        }
        Ok(self.named_function_stage(name, body, declared, Vec::new()))
    }

    /// Stage `bound.len()` of a curried named-function closure: binds
    /// the next input and captures the arguments bound so far. A
    /// literal's explicit parameter precedes its captures, so input 0
    /// is this stage's argument and inputs 1.. are the earlier ones.
    fn named_function_stage(
        &mut self,
        name: &str,
        body: &EmirProgram,
        declared: &[String],
        bound: Vec<EmirValue>,
    ) -> EmirValue {
        let depth = bound.len();
        let mut child = self.capture_child();
        child.inputs = (0..=depth)
            .map(|index| format!("__named_arg{index}"))
            .collect();
        let own = child.push(EmirOp::LoadInput(0));
        let mut args: Vec<EmirValue> = (1..=depth)
            .map(|index| child.push(EmirOp::LoadInput(index as u16)))
            .collect();
        args.push(own);
        let result = if depth + 1 == declared.len() {
            child.push(EmirOp::CallFrame {
                body: body.clone(),
                inputs: args,
                state: Vec::new(),
                declared: declared.to_vec(),
                callee: Some(name.to_string()),
            })
        } else {
            child.named_function_stage(name, body, declared, args)
        };
        let program = child.finish(result);
        self.push(EmirOp::ProgramLiteral {
            body: program,
            captures: bound,
            vector_input: false,
            signature: declared[depth].clone(),
        })
    }

    /// `quote.evaluate(code)` whose value is used as a function: the
    /// evaluated quoted function is a callable (the VM's closure; its
    /// body runs per application) of the application's carrier.
    fn lower_applied_evaluate(
        &mut self,
        code: &Expr,
        applied: CodeApplication,
    ) -> Result<EmirValue, String> {
        let code = self.expr(code)?;
        Ok(self.push(EmirOp::CodeEvaluate {
            code,
            applied: Some(applied),
        }))
    }

    fn lower_named_call(
        &mut self,
        segments: &[String],
        args: &[Expr],
    ) -> Result<EmirValue, String> {
        let called = segments.join(".");
        if called == "length" && args.len() == 1 {
            let value = self.expr(&args[0])?;
            return Ok(self.push(EmirOp::VectorLength(value)));
        }
        // Fence: buffer-carrier
        // ops are constructor-VM machine seams, not emitted math. The
        // named refusal keeps emission honest instead of emitting a
        // call the artifact cannot honor.
        if machine_buffer_basename(&called).is_some() {
            return Err(
                "buffer carrier ops run in the constructor VM; they are not emitted in this cut"
                    .into(),
            );
        }
        if let Some(op) = machine_int_basename(&called) {
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::ExactIntCall {
                name: op.to_string(),
                args: inputs,
            }));
        }
        if called == "constructor_refuse" {
            // Intentional refusal ABI in emission: a quoted single
            // identifier names the refusal code, and the evaluating
            // branch refuses at runtime with that code (the same named
            // fault the constructor VM raises). Any other shape stays
            // refused - a refusal is never emitted as a value.
            if let Some(code) = quoted_reason(args) {
                return Ok(self.push(EmirOp::Refuse(code)));
            }
            return Err(
                "constructor_refuse is an intentional refusal ABI; it is not emitted as a successful call"
                    .into(),
            );
        }
        if called == "transformation_rule_unavailable" {
            // The VM's opaque-operation refusal identity (call.rs):
            // authored transformation modules name their boundary
            // with it in the unexercised else-branches. In emission it
            // is the same named runtime fault - a branch that actually
            // takes it refuses identically cross-lane, and the module
            // still lowers (the identity is not an unbound name).
            return Ok(self.push(EmirOp::Refuse(String::from(
                "transformation_rule_unavailable: opaque operation has no exposed transformation rule",
            ))));
        }
        if called.starts_with("quote.") {
            // Program-space quote calls: substitute binds one open
            // constant by partial application; evaluate is the
            // guarded executor yielding the specialized closure.
            // Every other quote.* spelling or arity keeps the fence.
            return match (called.as_str(), args.len()) {
                ("quote.substitute", 3) => {
                    // The reference is static (the VM's authored
                    // spellings): a string literal or a
                    // single-segment name path. A runtime reference
                    // rides as a node-family operand - the
                    // minted-scope packaging spells binder names as
                    // tag records whose type name IS the name (the
                    // VM's dynamic reference law).
                    let reference = match &args[1].kind {
                        ExprKind::Str(reference) => CodeSubRef::Static(reference.clone()),
                        ExprKind::Path { segments, .. } if segments.len() == 1 => {
                            CodeSubRef::Static(segments[0].clone())
                        }
                        _ => CodeSubRef::Dyn(self.expr(&args[1])?),
                    };
                    let code = self.expr(&args[0])?;
                    let value = self.expr(&args[2])?;
                    Ok(self.push(EmirOp::CodeSubstitute {
                        code,
                        reference,
                        value,
                    }))
                }
                ("quote.evaluate", 1) => {
                    let code = self.expr(&args[0])?;
                    Ok(self.push(EmirOp::CodeEvaluate {
                        code,
                        applied: None,
                    }))
                }
                // The shared-tree pair (bead
                // emath-shared-tree-view-make-bp8nu): view opens a
                // Code (or Fragment) into node records over the
                // embedded tree; make rebuilds a (possibly modified)
                // node-record tree back into Code.
                ("quote.view", 1) => {
                    let code = self.expr(&args[0])?;
                    Ok(self.push(EmirOp::CodeView { code }))
                }
                ("quote.make", 1) => {
                    let node = self.expr(&args[0])?;
                    Ok(self.push(EmirOp::CodeMake { node }))
                }
                // The definition-table unfold (bead
                // emath-quote-body-defs-trto7): a Code naming a
                // module function becomes the Available/Opaque body
                // record over the embedded definition table.
                ("quote.body", 1) => {
                    let code = self.expr(&args[0])?;
                    Ok(self.push(EmirOp::CodeBody { code }))
                }
                // The binder-half call form (bead
                // emath-quote-bind-open-consumer-6f86g): mint nested
                // binder syntax inside a code value and re-stamp the
                // dependencies - the identity over the distilled
                // subset. The binder FORM refuses by name below (a
                // fresh-tokened function literal is outside the
                // subset).
                ("quote.bind", 1) => {
                    let code = self.expr(&args[0])?;
                    Ok(self.push(EmirOp::CodeBind { code }))
                }
                ("quote.identity", 1) => {
                    let code = self.expr(&args[0])?;
                    Ok(self.push(EmirOp::CodeIdentity { code }))
                }
                ("quote.join", 2) => {
                    let left = self.expr(&args[0])?;
                    let right = self.expr(&args[1])?;
                    Ok(self.push(EmirOp::CodeJoin { left, right }))
                }
                _ => Err(format!("call is not yet emitted: {called}")),
            };
        }
        if segments.len() == 2 && segments[1] == "pack" {
            return self.lower_pack(&segments[0], args);
        }
        if segments.len() == 2 && segments[1] == "open" {
            return Err("open is a binder, not a plain call".into());
        }
        if self.self_name.as_deref() == Some(called.as_str()) {
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::CallSelf {
                inputs,
                result: self.self_result.clone(),
            }));
        }
        if let Some((declared, result)) = self.entry_siblings.get(&called).cloned() {
            // The cycle edge: the callee is an open ancestor (A calls
            // B, B calls A), so its body cannot inline. Lower the
            // named sibling call - the backend renders a call to the
            // callee's emitted entry fn with the declared carriers.
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::CallSibling {
                name: called.clone(),
                inputs,
                declared,
                result,
            }));
        }
        if let Some((body, declared)) = self.siblings.get(&called).cloned() {
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::CallFrame {
                body,
                inputs,
                state: Vec::new(),
                declared,
                callee: Some(called),
            }));
        }
        // A callee bound in the enclosing frame is a closure value:
        // emit a typed indirect call. Curried programs fold application
        // left, so `probe(k, cs)` on `Int -> CaseSet -> Rat` is one op.
        // The arrow-name gate keeps field access (`base.field`, which
        // parses as this same one-argument call shape) from shadowing:
        // only closure-valued names (arrow-declared inputs, function
        // literal defs, arrow-domain params) take this arm.
        if self.arrow_names.contains(&called)
            && let Some(program) = self.locals.get(&called).copied().or_else(|| {
                self.inputs
                    .iter()
                    .position(|input| input == &called)
                    .map(|index| self.push(EmirOp::LoadInput(index as u16)))
            })
        {
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            if inputs.is_empty() {
                return Err(format!("call `{called}` requires at least one argument"));
            }
            return Ok(self.push(EmirOp::CallValue { program, inputs }));
        }
        // `base.field` parses as `Call { Path[field], [base] }`: a
        // one-argument call on a name that is not closure-valued is
        // record projection. (A bound non-arrow callee cannot be a
        // call: the engine faults calling a non-function.)
        if segments.len() == 1 && args.len() == 1 && !self.arrow_names.contains(&called) {
            let record = self.expr(&args[0])?;
            return Ok(self.push(EmirOp::RecordField {
                record,
                field: called,
            }));
        }
        // `r.add(x)` with `r` bound in this frame: the projected field
        // is a closure value (a record's arrow-typed field), called
        // indirectly like an arrow-named input.
        if segments.len() >= 2
            && !args.is_empty()
            && (self.locals.contains_key(&segments[0]) || self.inputs.contains(&segments[0]))
        {
            let program = self.lookup_path(segments)?;
            let mut inputs = Vec::new();
            for arg in args {
                inputs.push(self.expr(arg)?);
            }
            return Ok(self.push(EmirOp::CallValue { program, inputs }));
        }
        Err(format!("call is not yet emitted: {called}"))
    }

    fn lower_pack(&mut self, type_name: &str, args: &[Expr]) -> Result<EmirValue, String> {
        let kind = self
            .objects
            .get(type_name)
            .cloned()
            .ok_or_else(|| format!("unknown object `{type_name}`"))?;
        match kind.as_str() {
            "abstract" => {
                if args.len() != 1 {
                    return Err("abstract pack expects one representation".into());
                }
                let repr = self.expr(&args[0])?;
                Ok(self.push(EmirOp::RecordCreate {
                    type_name: type_name.to_string(),
                    fields: vec![("repr".into(), repr)],
                }))
            }
            "package" => {
                if args.len() < 2 {
                    return Err("package pack expects n and data".into());
                }
                if let (Some(want), Some(got)) = (static_int(&args[0]), static_list_len(&args[1])) {
                    if want != got {
                        return Err(format!("invalid_index: T[n] pack length {got} != {want}"));
                    }
                }
                let n = self.expr(&args[0])?;
                let data = self.expr(&args[1])?;
                Ok(self.push(EmirOp::RecordCreate {
                    type_name: type_name.to_string(),
                    fields: vec![("n".into(), n), ("data".into(), data)],
                }))
            }
            other => Err(format!(
                "`{type_name}.pack` requires abstract or package representation, found {other}"
            )),
        }
    }

    fn lower_open(
        &mut self,
        type_name: &str,
        param: &str,
        packed: &Expr,
        body: &Expr,
    ) -> Result<EmirValue, String> {
        let kind = self
            .objects
            .get(type_name)
            .cloned()
            .ok_or_else(|| format!("unknown object `{type_name}`"))?;
        let packed = self.expr(packed)?;
        let opened = if kind == "abstract" {
            self.push(EmirOp::RecordField {
                record: packed,
                field: "repr".into(),
            })
        } else {
            packed
        };
        let previous = self.locals.insert(param.to_string(), opened);
        let result = self.expr(body);
        match previous {
            Some(value) => {
                self.locals.insert(param.to_string(), value);
            }
            None => {
                self.locals.remove(param);
            }
        }
        result
    }

    fn finish(self, result: EmirValue) -> EmirProgram {
        EmirProgram {
            ops: self.ops,
            result,
            input_count: u16::try_from(self.inputs.len()).expect("input count"),
            state_count: 0,
            domain_obligations: self.obligations,
        }
    }

    fn expr(&mut self, expr: &Expr) -> Result<EmirValue, String> {
        match &expr.kind {
            ExprKind::Int(text) => Ok(self.push(const_exact_int(text)?)),
            ExprKind::Bool(value) => Ok(self.push(EmirOp::ConstBool(*value))),
            // A Float64 literal lowers to the f64 constant op
            // (bit-exact through the whole emitted lane). The suffix
            // strip and parse are the constructor VM's literal law
            // (engine_step/core.rs) verbatim: both lanes must read
            // the same text the same way.
            ExprKind::Float(text) => {
                let cleaned = text.trim_end_matches("f64").trim_end_matches("Float64");
                let value = cleaned
                    .parse::<f64>()
                    .map_err(|_| format!("float literal is not Float64: {text}"))?;
                Ok(self.push(EmirOp::ConstF64(value.to_bits())))
            }
            // A Str literal lowers to the text constant op (e6gvs): the
            // carrier evaluates in the emitted program exactly as it
            // does in the constructor VM — dropping the arm demoted
            // every Str-bearing module to symbolic (not-runnable).
            ExprKind::Str(text) => Ok(self.push(EmirOp::ConstText(text.clone()))),
            ExprKind::Rational { numer, denom } => {
                let num = self.push(const_exact_int(numer)?);
                let den = self.push(const_exact_int(denom)?);
                self.obligations.push(DomainObligation::DivisionNonZero);
                Ok(self.push(EmirOp::F64Div(num, den)))
            }
            ExprKind::Path { segments, .. } => self.lookup_path(segments),
            ExprKind::Unary { op, value } => {
                let value = self.expr(value)?;
                match op {
                    UnaryOp::Neg => Ok(self.push(EmirOp::Neg(value))),
                    UnaryOp::Not => Ok(self.push(EmirOp::Not(value))),
                    UnaryOp::Pos => Ok(value),
                }
            }
            ExprKind::Binary { op, left, right } => {
                let left = self.expr(left)?;
                let right = self.expr(right)?;
                let opcode = match op {
                    BinaryOp::Add => EmirOp::F64Add(left, right),
                    BinaryOp::Sub => EmirOp::F64Sub(left, right),
                    BinaryOp::Mul => EmirOp::F64Mul(left, right),
                    BinaryOp::Div => {
                        self.obligations.push(DomainObligation::DivisionNonZero);
                        EmirOp::F64Div(left, right)
                    }
                    BinaryOp::Eq => EmirOp::Eq(left, right),
                    BinaryOp::Ne => EmirOp::Ne(left, right),
                    BinaryOp::Lt => EmirOp::Lt(left, right),
                    BinaryOp::Le => EmirOp::Le(left, right),
                    BinaryOp::Gt => EmirOp::Gt(left, right),
                    BinaryOp::Ge => EmirOp::Ge(left, right),
                    BinaryOp::And => EmirOp::And(left, right),
                    BinaryOp::Or => EmirOp::Or(left, right),
                    other => {
                        return Err(format!(
                            "operator {other:?} is not a scalar carrier operation"
                        ));
                    }
                };
                Ok(self.push(opcode))
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expr(condition)?;
                let then_body = self.nested(then_value)?;
                let else_body = self.nested(else_value)?;
                let args = self.captured_args();
                Ok(self.push(EmirOp::Branch {
                    condition,
                    args,
                    then_body,
                    else_body,
                }))
            }
            ExprKind::Call { function, args } => match &function.kind {
                ExprKind::Recur { name, body, .. } => self.lower_recur_apply(name, body, args),
                ExprKind::FunctionAbs {
                    param,
                    domain,
                    body,
                } => {
                    let (nested, captures) = self.lower_function_abs(param, domain, body)?;
                    let mut inputs = Vec::new();
                    for arg in args {
                        inputs.push(self.expr(arg)?);
                    }
                    // Immediate application: the literal frame takes the
                    // call arguments first, then the closure's captures.
                    let capture_count = captures.len();
                    inputs.extend(captures);
                    // The parameter's authored domain declares the
                    // first frame input; captures carry their own
                    // parent-frame kinds.
                    let mut declared = vec![
                        domain_signature(domain, &self.objects, &self.type_vars)
                            .unwrap_or_default(),
                    ];
                    declared.extend(std::iter::repeat_n(String::new(), capture_count));
                    Ok(self.push(EmirOp::CallFrame {
                        body: nested,
                        inputs,
                        state: Vec::new(),
                        declared,
                        callee: None,
                    }))
                }
                ExprKind::Path { segments, .. } => self.lower_named_call(segments, args),
                other => {
                    // A callee that is itself an expression: the
                    // field-access call `base.guard(args...)` parses
                    // as an application of the field-access call
                    // (`Call { function: Call { Path[guard],
                    // [base] }, args }`), and `f(x)(y)` applies a
                    // call's result. The VM's law is general - the
                    // callee is a VALUE (the projected field, the
                    // call's result) and the arguments apply stage
                    // by stage through its signature - so the
                    // expression lowers as a value and the arguments
                    // cross through the closure-call lane. A callee
                    // without a callable carrier refuses by name at
                    // the render boundary (the same fence the direct
                    // closure calls keep).
                    // `quote.evaluate(code)(args)`: the evaluated
                    // quoted function applied in place.
                    let program = match &other {
                        ExprKind::Call {
                            function: inner,
                            args: inner_args,
                        } if is_quote_evaluate_call(inner) && inner_args.len() == 1 => {
                            let arity = u16::try_from(args.len()).map_err(|_| {
                                String::from("quote.evaluate applied to too many arguments")
                            })?;
                            self.lower_applied_evaluate(
                                &inner_args[0],
                                CodeApplication::Union(arity),
                            )?
                        }
                        _ => self.expr(function)?,
                    };
                    let mut inputs = Vec::new();
                    for arg in args {
                        inputs.push(self.expr(arg)?);
                    }
                    if inputs.is_empty() {
                        return Err(format!("call is not yet emitted: {other:?}"));
                    }
                    Ok(self.push(EmirOp::CallValue { program, inputs }))
                }
            },
            ExprKind::FunctionAbs {
                param,
                domain,
                body,
            } => {
                let (nested, captures) = self.lower_function_abs(param, domain, body)?;
                // An unmapped domain keeps the empty numeric-lane
                // signature (interp-verified carrier), matching the
                // pre-typed-ABI behavior instead of refusing lowering.
                let signature =
                    domain_signature(domain, &self.objects, &self.type_vars).unwrap_or_default();
                Ok(self.push(EmirOp::ProgramLiteral {
                    body: nested,
                    captures,
                    vector_input: false,
                    signature,
                }))
            }
            ExprKind::Quote { body } => {
                // Quote emission: two template shapes share one
                // predicate (admission and the unresolved walk can
                // never disagree). A FUNCTION template wraps a unary
                // scalar-carrier program; its body's free names
                // beyond the parameter are the OPEN constants - the
                // hygiene law keeps them open (a quote never captures
                // the ambient frame), so they become the template's
                // runtime substitution inputs. An EXPRESSION
                // template (bead emath-expression-quotes-324y0) is a
                // quoted expression with free names and no wrapper:
                // no parameter, every free name a runtime input, the
                // carrier dynamic (`Union`) - the body compiles over
                // the value union and `evaluate` yields a scalar
                // projected at typed boundaries. The union lane also
                // distills the SAME authored body into the shared
                // tree (bead emath-shared-tree-view-make-bp8nu): the
                // dual representation - compiled factory plus data
                // tree - emitted by this one pass, and the capture
                // stamps free names that resolve against the module
                // callable table exactly as the VM's quote capture
                // does (`dependency_snapshot`).
                let Some(carrier) = emitted_quote_carrier(body) else {
                    return Err(
                        "quote emission supports unary Int/Rat/Bool/Float64 function templates and scalar expression templates in this cut"
                            .into(),
                    );
                };
                if let ExprKind::FunctionAbs {
                    param, body: inner, ..
                } = &body.kind
                    && carrier != "Union"
                {
                    let mut free = BTreeSet::new();
                    collect_free_names(inner, param, &mut free);
                    // A module function CALLED in the body is a
                    // dependency, not an open constant (the VM's capture
                    // law): the compiled template calls it as a sibling.
                    // Other free names keep the open-constant lane.
                    let mut called = BTreeMap::new();
                    collect_call_sites(inner, &mut called);
                    let free: Vec<String> = free
                        .into_iter()
                        .filter(|name| {
                            !(self.module_stamps.contains_key(name) && called.contains_key(name))
                        })
                        .collect();
                    let mut inputs = vec![param.clone()];
                    inputs.extend(free.iter().cloned());
                    let mut child = Lowerer {
                        inputs,
                        locals: BTreeMap::new(),
                        siblings: self.siblings.clone(),
                        entry_siblings: self.entry_siblings.clone(),
                        objects: self.objects.clone(),
                        ops: Vec::new(),
                        obligations: Vec::new(),
                        self_name: None,
                        self_result: String::new(),
                        // Empty arrow names: the open constants are
                        // data inputs, not callables, and the
                        // parameter is the template's only binder.
                        arrow_names: BTreeSet::new(),
                        module_stamps: self.module_stamps.clone(),
                        type_vars: self.type_vars.clone(),
                        hints: self.hints.clone(),
                    };
                    let lowered = child.expr(inner)?;
                    let program = child.finish(lowered);
                    // The dual representation: the compiled factory
                    // AND the distilled Closure tree (the view seam
                    // reads a program template's structure through
                    // it, the same dual-rep the union lane carries).
                    // Dependency stamping stays a union-lane fact.
                    // The distillation is soft: a template outside
                    // the distilled subset (float carriers) keeps
                    // the factory lane and simply has no tree to
                    // view - `quote.view` refuses by name at runtime
                    // instead of poisoning the function's
                    // runnability.
                    Ok(self.push(EmirOp::CodeLiteral {
                        body: program,
                        param: Some(param.clone()),
                        free,
                        carrier: carrier.to_string(),
                        tree: crate::tree_distill::distill_tree(body).ok(),
                        deps: BTreeMap::new(),
                    }))
                } else {
                    // The expression template: all free names are
                    // runtime inputs (no parameter to exclude), the
                    // carrier is the dynamic union. The distillation
                    // and the factory lower over the same body in
                    // this same arm - the dual-representation law.
                    let mut free = BTreeSet::new();
                    collect_free_names(body, "", &mut free);
                    let free: Vec<String> = free.into_iter().collect();
                    let tree = crate::tree_distill::distill_tree(body)?;
                    let deps: BTreeMap<String, u64> = free
                        .iter()
                        .filter_map(|name| {
                            self.module_stamps
                                .get(name)
                                .map(|stamp| (name.clone(), *stamp))
                        })
                        .collect();
                    let mut child = Lowerer {
                        inputs: free.clone(),
                        locals: BTreeMap::new(),
                        siblings: self.siblings.clone(),
                        entry_siblings: self.entry_siblings.clone(),
                        objects: self.objects.clone(),
                        ops: Vec::new(),
                        obligations: Vec::new(),
                        self_name: None,
                        self_result: String::new(),
                        arrow_names: BTreeSet::new(),
                        module_stamps: self.module_stamps.clone(),
                        type_vars: self.type_vars.clone(),
                        hints: self.hints.clone(),
                    };
                    // A body the compiled lane cannot lower (a recursive
                    // function literal) keeps its tree alone: the
                    // refusing body has no union kind, so the backend
                    // evaluates the template through the tree evaluator.
                    let lowered = match child.expr(body) {
                        Ok(lowered) => lowered,
                        Err(detail) => {
                            child.ops.clear();
                            child.obligations.clear();
                            child.push(EmirOp::Refuse(detail))
                        }
                    };
                    let program = child.finish(lowered);
                    Ok(self.push(EmirOp::CodeLiteral {
                        body: program,
                        param: None,
                        free,
                        carrier: carrier.to_string(),
                        tree: Some(tree),
                        deps,
                    }))
                }
            }
            ExprKind::List(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.push(self.expr(item)?);
                }
                Ok(self.push(EmirOp::ListCreate(values)))
            }
            ExprKind::Tuple(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.push(self.expr(item)?);
                }
                Ok(self.push(EmirOp::ListCreate(values)))
            }
            ExprKind::Record { type_path, fields } => {
                // Authored record literal: the type name is the final
                // path segment (module-qualified spellings install the
                // bare name), fields stay in authored order. An
                // authored OBJECT admits as the typed record lane; a
                // node-family tag (the engine's schema-tag family:
                // `Call`, `Literal`, ...) admits as the dynamic node
                // record the structural walk constructs - the same
                // names the VM's `finish_record` accepts as plain
                // records. Anything else refuses by name.
                let type_name = type_path
                    .last()
                    .cloned()
                    .ok_or_else(|| "record literal requires a type path".to_string())?;
                if !self.objects.contains_key(&type_name)
                    && !crate::constructor_layer::is_node_tag(&type_name)
                {
                    return Err(format!(
                        "record `{type_name}` is not an authored object or a node tag"
                    ));
                }
                let mut values = Vec::with_capacity(fields.len());
                let hints = std::rc::Rc::clone(&self.hints);
                for (name, field) in fields {
                    let value = match hints
                        .object_fields
                        .get(&type_name)
                        .and_then(|fields| fields.get(name))
                    {
                        Some(expected) => self.expr_expecting(field, expected)?,
                        None => self.expr(field)?,
                    };
                    values.push((name.clone(), value));
                }
                Ok(self.push(EmirOp::RecordCreate {
                    type_name,
                    fields: values,
                }))
            }
            ExprKind::SequenceCons {
                head,
                tail,
                spread_head,
            } => {
                // `[head, ..tail]`: cons as a one-element list
                // concatenated onto the tail (`[..head, ..tail]`
                // concatenates the head sequence itself). ListConcat
                // preserves element carriers (records stay records),
                // unlike the Float64 dense-lane VectorConcat.
                let head = self.expr(head)?;
                let tail = self.expr(tail)?;
                let front = if *spread_head {
                    head
                } else {
                    self.push(EmirOp::ListCreate(vec![head]))
                };
                Ok(self.push(EmirOp::ListConcat(vec![front, tail])))
            }
            ExprKind::Index { value, indices } => {
                let vector = self.expr(value)?;
                let Some(index) = indices.first() else {
                    return Err("index requires at least one index".into());
                };
                let index = self.expr(index)?;
                Ok(self.push(EmirOp::VectorIndex { vector, index }))
            }
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => match &callee.kind {
                // The binder half of the tree family (bead
                // emath-quote-bind-open-consumer-6f86g):
                // `quote.open term in package: body` lowers as the
                // witness-validated unwrap (CodeOpen) bound to the
                // parameter for the body, mirroring lower_open's
                // local-binding shape.
                ExprKind::Path { segments, .. }
                    if segments.len() == 2 && segments[0] == "quote" && segments[1] == "open" =>
                {
                    let package = self.expr(domain)?;
                    let opened = self.push(EmirOp::CodeOpen { package });
                    let previous = self.locals.insert(param.clone(), opened);
                    let result = self.expr(body);
                    match previous {
                        Some(value) => {
                            self.locals.insert(param.clone(), value);
                        }
                        None => {
                            self.locals.remove(param);
                        }
                    }
                    result
                }
                // `quote.bind p in domain: body` mints a
                // fresh-tokened function literal - a binder node
                // outside the distilled artifact subset. The named
                // refusal is the honest boundary (the VM lane keeps
                // the full algorithm; the artifact tree carries no
                // binder nodes).
                ExprKind::Path { segments, .. }
                    if segments.len() == 2 && segments[0] == "quote" && segments[1] == "bind" =>
                {
                    Err(String::from(
                        "quote.bind binder form is not emitted in artifact trees: a fresh-tokened function literal is outside the distilled subset",
                    ))
                }
                ExprKind::Path { segments, .. } if segments.len() == 2 && segments[1] == "open" => {
                    self.lower_open(&segments[0], param, domain, body)
                }
                _ => Err(format!(
                    "expression form is not yet emitted: {:?}",
                    expr.kind
                )),
            },
            ExprKind::Cases { arms, else_arm, .. } => self.lower_cases(arms, else_arm),
            ExprKind::Recur { name, body, .. } => {
                if let ExprKind::FunctionAbs {
                    param, body: inner, ..
                } = &body.kind
                {
                    if let Some(program) = lower_int_primitive_recur(name, param, inner) {
                        return Ok(self.push(EmirOp::program_literal(program)));
                    }
                    if contains_self_call(inner, name) {
                        let program = lower_general_recur(name, param, inner)?;
                        return Ok(self.push(EmirOp::program_literal(program)));
                    }
                }
                if contains_self_call(body, name) {
                    return Err(format!("recursive `{name}` is not a scalar emission yet"));
                }
                self.expr(body)
            }
            other => Err(format!("expression form is not yet emitted: {other:?}")),
        }
    }
}

fn lower_closed(
    inputs: &[String],
    body: &Expr,
    self_name: Option<String>,
) -> Result<EmirProgram, String> {
    let mut lowerer = Lowerer {
        inputs: inputs.to_vec(),
        locals: BTreeMap::new(),
        siblings: std::rc::Rc::default(),
        entry_siblings: BTreeMap::new(),
        objects: BTreeMap::new(),
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name,
        self_result: String::new(),
        arrow_names: BTreeSet::new(),
        module_stamps: BTreeMap::new(),
        type_vars: Vec::new(),
        hints: std::rc::Rc::default(),
    };
    let result = lowerer.expr(body)?;
    Ok(lowerer.finish(result))
}

fn lower_general_recur(name: &str, param: &str, body: &Expr) -> Result<EmirProgram, String> {
    let inputs = vec![param.to_string()];
    lower_closed(&inputs, body, Some(name.to_string()))
}

fn expr_uses_quote(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Quote { .. } | ExprKind::QuoteBind { .. } => true,
        ExprKind::Call { function, args } => {
            if is_refuse_quote_call(function, args) {
                // `constructor_refuse(quote(code))` is emitted as a
                // named runtime refusal, not residualized as a quote
                // transform.
                return false;
            }
            if let ExprKind::Path { segments, .. } = &function.kind {
                if segments.first().map(String::as_str) == Some("quote") {
                    return true;
                }
            }
            expr_uses_quote(function) || args.iter().any(expr_uses_quote)
        }
        ExprKind::CallableBinder {
            callee,
            domain,
            body,
            ..
        } => {
            if let ExprKind::Path { segments, .. } = &callee.kind {
                if segments.first().map(String::as_str) == Some("quote") {
                    return true;
                }
            }
            expr_uses_quote(callee) || expr_uses_quote(domain) || expr_uses_quote(body)
        }
        ExprKind::Unary { value, .. } => expr_uses_quote(value),
        ExprKind::Binary { left, right, .. } => expr_uses_quote(left) || expr_uses_quote(right),
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            expr_uses_quote(condition) || expr_uses_quote(then_value) || expr_uses_quote(else_value)
        }
        ExprKind::FunctionAbs { body, domain, .. }
        | ExprKind::Recur {
            body, ty: domain, ..
        } => expr_uses_quote(domain) || expr_uses_quote(body),
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            subject
                .as_ref()
                .is_some_and(|subject| expr_uses_quote(subject))
                || arms
                    .iter()
                    .any(|(cond, value)| expr_uses_quote(cond) || expr_uses_quote(value))
                || expr_uses_quote(else_arm)
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => items.iter().any(expr_uses_quote),
        ExprKind::Index { value, indices } => {
            expr_uses_quote(value) || indices.iter().any(expr_uses_quote)
        }
        ExprKind::Record { fields, .. } => fields.iter().any(|(_, value)| expr_uses_quote(value)),
        _ => false,
    }
}

/// The emitted quote-template shape and its declared scalar carrier:
/// a unary function literal over `Int`, `Rat`, or `Bool` (bare domain
/// spelling). One authority for both the lowering admission and the
/// unresolved exemption, so the two can never disagree; every other
/// quote stays behind the fence as symbolic code.
fn emitted_quote_carrier(body: &Expr) -> Option<&'static str> {
    // An expression template (bead emath-expression-quotes-324y0): a
    // quoted expression with free names, no function wrapper. The
    // carrier of each free name is a substitute-time fact, so the
    // declared-carrier lane does not apply; the `Union` marker routes
    // the backend onto the dynamic value-union lane. Everything the
    // union lane cannot compute still refuses named (the body-kind
    // check and the scalar-op kind rules gate it).
    if !matches!(body.kind, ExprKind::FunctionAbs { .. }) {
        return Some("Union");
    }
    let ExprKind::FunctionAbs { domain, .. } = &body.kind else {
        return None;
    };
    // A function over sequences has no scalar-carrier factory; it is
    // carried as its tree alone, through the expression lane.
    if let ExprKind::Call { function, .. } = &domain.kind
        && matches!(&function.kind, ExprKind::Path { segments, .. } if segments.len() == 1 && segments[0] == "sequence")
    {
        return Some("Union");
    }
    let ExprKind::Path { segments, .. } = &domain.kind else {
        return None;
    };
    match segments.as_slice() {
        [segment] => match segment.as_str() {
            "Int" => Some("Int"),
            "Rat" => Some("Rat"),
            "Bool" => Some("Bool"),
            "Float64" => Some("Float64"),
            _ => None,
        },
        _ => None,
    }
}

/// `constructor_refuse(quote(name))`: the authored single-identifier
/// reason carried by the quote, if the argument is exactly that shape.
fn quoted_reason(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::Quote { body } => match &body.kind {
            ExprKind::Path { segments, .. } if segments.len() == 1 => Some(segments[0].clone()),
            _ => None,
        },
        _ => None,
    }
}

/// True when the call is exactly `constructor_refuse(quote(name))` - the
/// one quote shape emission handles directly (a named runtime refusal).
fn is_refuse_quote_call(function: &Expr, args: &[Expr]) -> bool {
    let ExprKind::Path { segments, .. } = &function.kind else {
        return false;
    };
    segments.len() == 1 && segments[0] == "constructor_refuse" && quoted_reason(args).is_some()
}

/// Carrier signature for a binder-domain expression: `Int` -> `Int`,
/// an object name -> `Record<Name>`, `sequence(T)` -> `Vector<T>`, and
/// the arrow bridge `A -> B` (parsed as `Call { Path[Fn], [A, B] }`)
/// -> `Fn<A,B>`. None means the domain has no native carrier.
fn domain_signature(
    domain: &Expr,
    objects: &BTreeMap<String, String>,
    vars: &[String],
) -> Option<String> {
    match &domain.kind {
        ExprKind::Path { segments, .. } => {
            let name = segments.last()?;
            if segments.len() == 1 && vars.contains(name) {
                return Some(format!("Var<{name}>"));
            }
            match name.as_str() {
                "Int" | "Rat" | "Bool" | "Text" | "Float64" => Some(name.clone()),
                // A literal's `Code` binder admits any value at run
                // time in the VM (its domain check passes everything
                // but the numeric carriers), viewed records included,
                // so it binds the dynamic node family.
                "Code" | "Fragment" => Some("Fragment".into()),
                _ => objects
                    .contains_key(name)
                    .then(|| format!("Record<{name}>")),
            }
        }
        // The bracket spelling `sequence[T]` (a type in expression
        // position parses as an index) names the same carrier as
        // `sequence(T)`.
        ExprKind::Index { value, indices } => {
            let ExprKind::Path { segments, .. } = &value.kind else {
                return None;
            };
            let [element] = indices.as_slice() else {
                return None;
            };
            (segments.last()? == "sequence")
                .then(|| domain_signature(element, objects, vars))
                .flatten()
                .map(|element| format!("Vector<{element}>"))
        }
        ExprKind::Call { function, args } => {
            let ExprKind::Path { segments, .. } = &function.kind else {
                return None;
            };
            match segments.last()?.as_str() {
                // Arrow bridge: every part but the last is a domain.
                "Fn" => {
                    let parts = args
                        .iter()
                        .map(|arg| domain_signature(arg, objects, vars))
                        .collect::<Option<Vec<_>>>()?;
                    let (result, domains) = parts.split_last()?;
                    Some(format!("Fn<{},{}>", domains.join(","), result))
                }
                "sequence" => {
                    let [element] = args.as_slice() else {
                        return None;
                    };
                    Some(format!(
                        "Vector<{}>",
                        domain_signature(element, objects, vars)?
                    ))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Free path-head names of a closure body, excluding the closure's own
/// parameter. Nested literals bind their own params, so only true
/// enclosing names surface (the capture candidates).
/// `A -> B` in a domain position parses as `Call { Path[Fn], [A, B] }`
/// (the parser's fn-arrow bridge); the literal's parameter is itself
/// callable when its domain has that shape.
fn is_arrow_domain(domain: &Expr) -> bool {
    matches!(
        &domain.kind,
        ExprKind::Call { function, .. }
            if matches!(
                &function.kind,
                ExprKind::Path { segments, .. }
                    if segments.len() == 1 && segments[0] == "Fn"
            )
    )
}

fn collect_free_names(expr: &Expr, param: &str, out: &mut BTreeSet<String>) {
    let mut bound = BTreeSet::from([param.to_string()]);
    collect_free_names_bound(expr, &mut bound, out);
}

fn collect_free_names_bound(expr: &Expr, bound: &mut BTreeSet<String>, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Path { segments, .. } => {
            if let Some(head) = segments.first() {
                if !bound.contains(head) {
                    out.insert(head.clone());
                }
            }
        }
        ExprKind::Call { function, args } => {
            collect_free_names_bound(function, bound, out);
            for arg in args {
                collect_free_names_bound(arg, bound, out);
            }
        }
        ExprKind::Unary { value, .. } => collect_free_names_bound(value, bound, out),
        ExprKind::Binary { left, right, .. } => {
            collect_free_names_bound(left, bound, out);
            collect_free_names_bound(right, bound, out);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_free_names_bound(condition, bound, out);
            collect_free_names_bound(then_value, bound, out);
            collect_free_names_bound(else_value, bound, out);
        }
        ExprKind::FunctionAbs { param, body, .. } => {
            bound.insert(param.clone());
            collect_free_names_bound(body, bound, out);
            bound.remove(param);
        }
        // The declared type is a type expression, never a value
        // reference (the VM's `free_path_names`).
        ExprKind::Recur { name, body, .. } => {
            bound.insert(name.clone());
            collect_free_names_bound(body, bound, out);
            bound.remove(name);
        }
        ExprKind::QuoteBind {
            param,
            domain,
            body,
        } => {
            bound.insert(param.clone());
            collect_free_names_bound(domain, bound, out);
            collect_free_names_bound(body, bound, out);
            bound.remove(param);
        }
        ExprKind::CallableBinder {
            callee,
            param,
            domain,
            body,
        } => {
            bound.insert(param.clone());
            collect_free_names_bound(callee, bound, out);
            collect_free_names_bound(domain, bound, out);
            collect_free_names_bound(body, bound, out);
            bound.remove(param);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            if let Some(subject) = subject {
                collect_free_names_bound(subject, bound, out);
            }
            for (cond, value) in arms {
                collect_free_names_bound(cond, bound, out);
                collect_free_names_bound(value, bound, out);
            }
            collect_free_names_bound(else_arm, bound, out);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_free_names_bound(item, bound, out);
            }
        }
        ExprKind::Index { value, indices } => {
            collect_free_names_bound(value, bound, out);
            for index in indices {
                collect_free_names_bound(index, bound, out);
            }
        }
        ExprKind::Record { fields, .. } => {
            for (_, field) in fields {
                collect_free_names_bound(field, bound, out);
            }
        }
        ExprKind::SequenceCons { head, tail, .. } => {
            collect_free_names_bound(head, bound, out);
            collect_free_names_bound(tail, bound, out);
        }
        _ => {}
    }
}

fn collect_unresolved(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Quote { body } => {
            // The emitted template shape resolves (the Quote arm
            // lowers it); every other quote stays symbolic code.
            if emitted_quote_carrier(body).is_none() {
                out.push("quote".into());
            }
        }
        ExprKind::QuoteBind { .. } => {
            out.push("quote".into());
        }
        ExprKind::Call { function, args } => {
            if is_refuse_quote_call(function, args) {
                // The named-refusal quote is emitted (Refuse), so it is
                // not unresolved symbolic code.
                return;
            }
            collect_unresolved(function, out);
            for arg in args {
                collect_unresolved(arg, out);
            }
        }
        ExprKind::Unary { value, .. } => collect_unresolved(value, out),
        ExprKind::Binary { left, right, .. } => {
            collect_unresolved(left, out);
            collect_unresolved(right, out);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_unresolved(condition, out);
            collect_unresolved(then_value, out);
            collect_unresolved(else_value, out);
        }
        ExprKind::FunctionAbs { body, domain, .. } => {
            collect_unresolved(domain, out);
            collect_unresolved(body, out);
        }
        ExprKind::Recur { body, ty, .. } => {
            collect_unresolved(ty, out);
            collect_unresolved(body, out);
        }
        ExprKind::CallableBinder {
            callee,
            domain,
            body,
            ..
        } => {
            if let ExprKind::Path { segments, .. } = &callee.kind {
                let name = segments.join(".");
                // The quote.open binder form is emitted (CodeOpen);
                // every other quote binder stays symbolic code.
                if name != "quote.open" && name.starts_with("quote.") {
                    out.push("quote".into());
                }
            }
            collect_unresolved(callee, out);
            collect_unresolved(domain, out);
            collect_unresolved(body, out);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            if let Some(subject) = subject {
                collect_unresolved(subject, out);
            }
            for (cond, value) in arms {
                collect_unresolved(cond, out);
                collect_unresolved(value, out);
            }
            collect_unresolved(else_arm, out);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_unresolved(item, out);
            }
        }
        ExprKind::Index { value, indices } => {
            collect_unresolved(value, out);
            for index in indices {
                collect_unresolved(index, out);
            }
        }
        ExprKind::SequenceCons { head, tail, .. } => {
            collect_unresolved(head, out);
            collect_unresolved(tail, out);
        }
        // A record-literal field can carry a call (`SimState: {y:
        // combine(...)}`); without this arm the callee never joins
        // the sibling map.
        ExprKind::Record { fields, .. } => {
            for (_, field) in fields {
                collect_unresolved(field, out);
            }
        }
        _ => {}
    }
}

/// Every call through a dotted path callee reachable in an expression:
/// callee name -> one entry per call site, the site's arguments by
/// position (`Some(name)` where the argument is a bare local name).
/// Callee names build the sibling map; the sites give an evaluated
/// def's application arity and the declared arrows it is passed into.
fn collect_call_sites(expr: &Expr, out: &mut BTreeMap<String, Vec<Vec<Option<String>>>>) {
    match &expr.kind {
        ExprKind::Call { function, args } => {
            if let ExprKind::Path { segments, .. } = &function.kind {
                let site = args
                    .iter()
                    .map(|arg| match &arg.kind {
                        ExprKind::Path { segments, .. } if segments.len() == 1 => {
                            Some(segments[0].clone())
                        }
                        _ => None,
                    })
                    .collect();
                out.entry(segments.join(".")).or_default().push(site);
            }
            collect_call_sites(function, out);
            for arg in args {
                collect_call_sites(arg, out);
            }
        }
        ExprKind::Unary { value, .. } => collect_call_sites(value, out),
        ExprKind::Binary { left, right, .. } => {
            collect_call_sites(left, out);
            collect_call_sites(right, out);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_call_sites(condition, out);
            collect_call_sites(then_value, out);
            collect_call_sites(else_value, out);
        }
        ExprKind::FunctionAbs { body, domain, .. } => {
            collect_call_sites(domain, out);
            collect_call_sites(body, out);
        }
        ExprKind::Recur { body, ty, .. } => {
            collect_call_sites(ty, out);
            collect_call_sites(body, out);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            if let Some(subject) = subject {
                collect_call_sites(subject, out);
            }
            for (cond, value) in arms {
                collect_call_sites(cond, out);
                collect_call_sites(value, out);
            }
            collect_call_sites(else_arm, out);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_call_sites(item, out);
            }
        }
        ExprKind::Index { value, indices } => {
            collect_call_sites(value, out);
            for index in indices {
                collect_call_sites(index, out);
            }
        }
        // A binder body can carry a call (`quote.open term in packed:
        // walk_term(term)`); without this arm the callee never joins
        // the sibling map and the one-argument call falls through to
        // the field-access reading (`term.walk_term`).
        ExprKind::CallableBinder {
            callee,
            domain,
            body,
            ..
        } => {
            collect_call_sites(callee, out);
            collect_call_sites(domain, out);
            collect_call_sites(body, out);
        }
        // A cons head can carry a call (`[row_dot(tab, i, v), ..acc]`);
        // without this arm the callee never joins the sibling map and
        // the call falls through to the not-yet-emitted refusal.
        ExprKind::SequenceCons { head, tail, .. } => {
            collect_call_sites(head, out);
            collect_call_sites(tail, out);
        }
        ExprKind::Record { fields, .. } => {
            for (_, field) in fields {
                collect_call_sites(field, out);
            }
        }
        // A quoted body's calls name MODULE functions (the hygiene law:
        // a quote never sees the frame's locals), so its callees join
        // the sibling map - a compiled template calls them - but its
        // sites stay out of the frame's application arities.
        ExprKind::Quote { body } => {
            let mut quoted = BTreeMap::new();
            collect_call_sites(body, &mut quoted);
            for callee in quoted.into_keys() {
                out.entry(callee).or_default();
            }
        }
        _ => {}
    }
}

fn section_fields(decl: &Declaration, name: &str) -> Vec<String> {
    let mut names = Vec::new();
    for section in decl.sections().filter(|section| section.name == name) {
        for stmt in &section.suite.statements {
            if let StmtKind::FieldDecl { name, .. } = &stmt.kind {
                names.push(name.clone());
            }
        }
    }
    names
}

/// Section fields with their declared types (`inputs:`/`outputs:`
/// `FieldDecls`). The declared types are ground truth for the emitted
/// entry's parameter signature.
pub fn section_typed_fields(decl: &Declaration, name: &str) -> Vec<(String, TypeExpr)> {
    let mut fields = Vec::new();
    for section in decl.sections().filter(|section| section.name == name) {
        for stmt in &section.suite.statements {
            if let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                fields.push((name.clone(), ty.clone()));
            }
        }
    }
    fields
}

/// Declared type → carrier signature string, the interchange between
/// the constructor lane and the Rust backend's kind parser:
/// `Int`/`Nat` → "Int", `Rat` → "Rat", `Bool` → "Bool", `Float64` →
/// "Float64", `Text` → "Text", `Code` → "Code" (the union-lane
/// expression-template value: the dual tree/factory Code), an object
/// name → "Record<name>", `sequence(T)` → "Vector<T>", `A -> B` →
/// "Fn<A,B>". Unmapped forms return None - the caller refuses
/// emission rather than guessing a carrier.
pub fn constructor_type_signature(ty: &TypeExpr, objects: &BTreeSet<String>) -> Option<String> {
    type_signature(ty, objects, &[])
}

/// [`constructor_type_signature`] inside a generic declaration: each
/// of `vars` (its type parameters) renders as `Var<name>`.
pub fn generic_type_signature(
    ty: &TypeExpr,
    objects: &BTreeSet<String>,
    vars: &[String],
) -> Option<String> {
    type_signature(ty, objects, vars)
}

/// `vars` are the declaration's type parameters; each renders as
/// `Var<name>` so a caller can bind it from the arguments it passes.
fn type_signature(ty: &TypeExpr, objects: &BTreeSet<String>, vars: &[String]) -> Option<String> {
    match &ty.kind {
        TypeKind::Path {
            segments,
            generic_args,
        } => {
            let last = segments.last()?;
            if segments.len() == 1 && generic_args.is_empty() && vars.contains(last) {
                return Some(format!("Var<{last}>"));
            }
            match last.as_str() {
                "Int" | "Nat" => Some("Int".into()),
                "Rat" => Some("Rat".into()),
                "Bool" => Some("Bool".into()),
                "Float64" => Some("Float64".into()),
                "Text" => Some("Text".into()),
                // `Str` is the authored spelling of the text carrier
                // (e6gvs): the interchange signature stays `Text`, the
                // form the backend's kind parser already carries.
                "Str" => Some("Text".into()),
                // Bare `Code` is the expression template (open
                // expression with free names; evaluate yields the
                // union scalar). `Code[T]` names the program template's
                // scalar carrier: a quoted unary program over T
                // (substitutes bind T constants, evaluate yields the
                // specialized T -> T closure) - the backend's kind
                // parser splits the two on exactly this spelling.
                "Code" => match generic_args.first() {
                    Some(GenericArg::Type(carrier)) => {
                        Some(format!("Code<{}>", type_signature(carrier, objects, vars)?))
                    }
                    _ => Some("Code".into()),
                },
                // An unparameterized `sequence` is the VM's
                // heterogeneous list: its carrier is the dynamic node
                // family (`Sequence`), not a typed vector.
                "sequence" => match generic_args.first() {
                    None => Some("Sequence".into()),
                    Some(GenericArg::Type(element)) => Some(format!(
                        "Vector<{}>",
                        type_signature(element, objects, vars)?
                    )),
                    Some(_) => None,
                },
                // A Fragment package (a viewed child: term + minted
                // Scope witness) is a node-family record.
                "Fragment" => Some("Fragment".into()),
                // A generic object applied to carriers names its
                // instantiation (`Record<Pair<Int,Bool>>`): each
                // instantiation is its own layout.
                name if objects.contains(name) && !generic_args.is_empty() => {
                    let mut args = Vec::new();
                    for arg in generic_args {
                        let GenericArg::Type(arg) = arg else {
                            return None;
                        };
                        args.push(type_signature(arg, objects, vars)?);
                    }
                    Some(format!("Record<{name}<{}>>", args.join(",")))
                }
                name if objects.contains(name) => Some(format!("Record<{name}>")),
                _ => None,
            }
        }
        TypeKind::Tuple(elements) => {
            // A tuple domain kinds as the positional sequence its
            // literals lower to (ListCreate): the element carrier is
            // the join of the declared element domains - the same law
            // a heterogeneous tuple literal refuses under.
            let mut parts = Vec::new();
            for element in elements {
                parts.push(type_signature(element, objects, vars)?);
            }
            Some(format!("Tuple<{}>", parts.join(",")))
        }
        TypeKind::Fn { domain, codomain } => Some(format!(
            "Fn<{},{}>",
            type_signature(domain, objects, vars)?,
            type_signature(codomain, objects, vars)?
        )),
        _ => None,
    }
}

fn object_kinds(tree: &SyntaxTree) -> BTreeMap<String, String> {
    let mut kinds = BTreeMap::new();
    for item in &tree.items {
        if let Item::Declaration(decl) = item {
            if decl.as_kind == "object" {
                kinds.insert(decl.name.clone(), object_kind_of(decl));
            }
        }
    }
    kinds
}

fn object_kind_of(decl: &Declaration) -> String {
    for (name, expr) in section_assigns(decl, "representation") {
        if name == "kind" || name == "shape" {
            return match &expr.kind {
                ExprKind::Str(text) => text.clone(),
                ExprKind::Path { segments, .. } => {
                    segments.last().cloned().unwrap_or_else(|| "record".into())
                }
                _ => "record".into(),
            };
        }
    }
    "record".into()
}

fn static_int(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::Int(text) => text.replace('_', "").parse().ok(),
        _ => None,
    }
}

fn static_list_len(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::List(items) | ExprKind::Tuple(items) => i64::try_from(items.len()).ok(),
        _ => None,
    }
}

fn section_assigns(decl: &Declaration, name: &str) -> Vec<(String, Expr)> {
    let mut assigns = Vec::new();
    for section in decl.sections().filter(|section| section.name == name) {
        for stmt in &section.suite.statements {
            if let StmtKind::Assign { target, value } = &stmt.kind {
                if let Some(name) = target.segments.first() {
                    assigns.push((name.clone(), value.clone()));
                }
            }
        }
    }
    assigns
}

pub fn cvalue_to_emir(
    value: &crate::constructor_layer::CValue,
) -> Result<crate::interp::Value, String> {
    use crate::constructor_layer::CValue;
    use crate::interp::Value;
    match value {
        CValue::Bool(v) => Ok(Value::Bool(*v)),
        CValue::Str(text) => Ok(Value::Text(text.clone())),
        CValue::Int(v) => Ok(value_from_exact(v.clone())),
        CValue::Rat { num, den } => Ok(value_from_rat(num.clone(), den.clone())),
        CValue::Float64(v) => Ok(Value::F64(*v)),
        CValue::Sequence(items) => {
            let converted = items
                .iter()
                .map(cvalue_to_emir)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(converted))
        }
        CValue::Record { type_name, fields } => {
            let mut converted = BTreeMap::new();
            for (name, field) in fields.iter() {
                converted.insert(name.clone(), cvalue_to_emir(field)?);
            }
            Ok(Value::Record {
                type_name: type_name.to_string(),
                fields: converted,
            })
        }
        CValue::Closure(clos) => {
            // A closed function literal lowers to a typed program value
            // (the CallValue callee carrier). Closed means the body's
            // free names are all parameters: the engine's captured env
            // routinely holds enclosing-frame bindings the body never
            // reads, so the gate is free names, not env population. A
            // genuinely capturing or recursive closure stays in the
            // constructor VM.
            if clos.recursive.is_some() {
                return Err(
                    "recursive closures are not emitted as input carriers in this cut".into(),
                );
            }
            let mut free = BTreeSet::new();
            collect_free_names(&clos.body, &clos.param, &mut free);
            if !free.is_empty() {
                return Err(
                    "capturing closures are not emitted as input carriers in this cut".into(),
                );
            }
            let program = lower_closed(std::slice::from_ref(&clos.param), &clos.body, None)?;
            Ok(Value::Program(crate::interp::ProgramValue {
                body: program,
                captures: Vec::new(),
                vector_input: false,
            }))
        }
        CValue::Buffer(_) => Err(
            "buffer carrier values are not emitted in this cut; run in the constructor VM".into(),
        ),
        other => Err(format!("no EMIR carrier for {other}")),
    }
}

pub fn values_equal(
    left: &crate::interp::Value,
    right: &crate::constructor_layer::CValue,
) -> Result<bool, String> {
    let converted = cvalue_to_emir(right)?;
    Ok(emir_values_equal(left, &converted))
}

fn value_from_exact(value: ExactInt) -> crate::interp::Value {
    match value.to_i64() {
        Some(n) => crate::interp::Value::I64(n),
        None => crate::interp::Value::ExactInt(value),
    }
}

fn value_from_rat(num: ExactInt, den: ExactInt) -> crate::interp::Value {
    match (num.to_i128(), den.to_i128()) {
        (Some(num), Some(den)) => crate::interp::Value::Rat { num, den },
        _ => crate::interp::Value::ExactRat { num, den },
    }
}

fn const_exact_int(text: &str) -> Result<EmirOp, String> {
    let cleaned = text.replace('_', "");
    if let Ok(value) = cleaned.parse::<i64>() {
        return Ok(EmirOp::ConstI64(value));
    }
    ExactInt::parse(&cleaned)
        .map(|_| EmirOp::ConstExactInt(cleaned))
        .map_err(|_| format!("not Int: {text}"))
}

fn emir_values_equal(left: &crate::interp::Value, right: &crate::interp::Value) -> bool {
    match (left, right) {
        (crate::interp::Value::ExactInt(a), crate::interp::Value::ExactInt(b)) => a == b,
        (crate::interp::Value::ExactInt(a), crate::interp::Value::I64(b)) => {
            a == &ExactInt::from(*b)
        }
        (crate::interp::Value::I64(a), crate::interp::Value::ExactInt(b)) => {
            ExactInt::from(*a) == *b
        }
        (
            crate::interp::Value::ExactRat { num: an, den: ad },
            crate::interp::Value::ExactRat { num: bn, den: bd },
        ) => an
            .mul(bd)
            .is_ok_and(|left| bn.mul(ad).is_ok_and(|right| left == right)),
        (
            crate::interp::Value::ExactRat { num: an, den: ad },
            crate::interp::Value::Rat { num: bn, den: bd },
        )
        | (
            crate::interp::Value::Rat { num: bn, den: bd },
            crate::interp::Value::ExactRat { num: an, den: ad },
        ) => an
            .mul(&ExactInt::from(*bd))
            .is_ok_and(|left| ExactInt::from(*bn).mul(ad).is_ok_and(|right| left == right)),
        (
            crate::interp::Value::Rat { num: a, den: ad },
            crate::interp::Value::Rat { num: b, den: bd },
        ) => a * bd == b * ad,
        (crate::interp::Value::ExactInt(a), crate::interp::Value::Rat { num, den })
        | (crate::interp::Value::Rat { num, den }, crate::interp::Value::ExactInt(a)) => a
            .mul(&ExactInt::from(*den))
            .is_ok_and(|left| left == ExactInt::from(*num)),
        (crate::interp::Value::ExactInt(a), crate::interp::Value::ExactRat { num, den })
        | (crate::interp::Value::ExactRat { num, den }, crate::interp::Value::ExactInt(a)) => {
            a.mul(den).is_ok_and(|left| left == *num)
        }
        (crate::interp::Value::I64(a), crate::interp::Value::ExactRat { num, den })
        | (crate::interp::Value::ExactRat { num, den }, crate::interp::Value::I64(a)) => {
            ExactInt::from(*a).mul(den).is_ok_and(|left| left == *num)
        }
        (crate::interp::Value::I64(a), crate::interp::Value::Rat { num, den }) => {
            i128::from(*a) * *den == *num
        }
        (crate::interp::Value::Rat { num, den }, crate::interp::Value::I64(b)) => {
            *num == i128::from(*b) * *den
        }
        (crate::interp::Value::List(a), crate::interp::Value::List(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(left_item, right_item)| emir_values_equal(left_item, right_item))
        }
        (
            crate::interp::Value::Record { fields: a, .. },
            crate::interp::Value::Record { fields: b, .. },
        ) => {
            a.len() == b.len()
                && a.iter().all(|(name, value)| {
                    b.get(name)
                        .is_some_and(|other| emir_values_equal(value, other))
                })
        }
        (crate::interp::Value::Program(a), crate::interp::Value::Program(b)) => {
            program_values_equal(a, b)
        }
        _ => left == right,
    }
}

/// Structural equality for program carriers. A literal's `signature`
/// is an emission-ABI annotation that legitimately differs between
/// the authored-lowering lane and the closed-value conversion lane,
/// so it is ignored; bodies, captures, and packing must match.
fn program_values_equal(
    left: &crate::interp::ProgramValue,
    right: &crate::interp::ProgramValue,
) -> bool {
    left.vector_input == right.vector_input
        && left.captures.len() == right.captures.len()
        && programs_equal(&left.body, &right.body)
}

fn programs_equal(left: &EmirProgram, right: &EmirProgram) -> bool {
    left.input_count == right.input_count
        && left.state_count == right.state_count
        && left.result == right.result
        && left.ops.len() == right.ops.len()
        && left
            .ops
            .iter()
            .zip(&right.ops)
            .all(|((left_op, _), (right_op, _))| ops_equal(left_op, right_op))
}

fn ops_equal(left: &EmirOp, right: &EmirOp) -> bool {
    // Nested-body ops reach `programs_equal` so a `ProgramLiteral`
    // inside a body stays signature-blind across lanes; every
    // non-nested field still compares directly - two folds over the
    // same body with different `init` are different folds.
    match (left, right) {
        (
            EmirOp::ProgramLiteral {
                body: left_body,
                captures: left_captures,
                vector_input: left_vector,
                ..
            },
            EmirOp::ProgramLiteral {
                body: right_body,
                captures: right_captures,
                vector_input: right_vector,
                ..
            },
        ) => {
            left_vector == right_vector
                && left_captures == right_captures
                && programs_equal(left_body, right_body)
        }
        (
            EmirOp::CallFrame {
                body: left_body,
                inputs: left_inputs,
                state: left_state,
                ..
            },
            EmirOp::CallFrame {
                body: right_body,
                inputs: right_inputs,
                state: right_state,
                ..
            },
        ) => {
            left_inputs == right_inputs
                && left_state == right_state
                && programs_equal(left_body, right_body)
        }
        (
            EmirOp::Fold {
                body: left_body,
                start: left_start,
                end: left_end,
                init: left_init,
                combine: left_combine,
                loop_var_index: left_loop_var_index,
            },
            EmirOp::Fold {
                body: right_body,
                start: right_start,
                end: right_end,
                init: right_init,
                combine: right_combine,
                loop_var_index: right_loop_var_index,
            },
        ) => {
            left_start == right_start
                && left_end == right_end
                && left_init == right_init
                && left_combine == right_combine
                && left_loop_var_index == right_loop_var_index
                && programs_equal(left_body, right_body)
        }
        (
            EmirOp::Collect {
                count: left_count,
                args: left_args,
                body: left_body,
            },
            EmirOp::Collect {
                count: right_count,
                args: right_args,
                body: right_body,
            },
        ) => {
            left_count == right_count
                && left_args == right_args
                && programs_equal(left_body, right_body)
        }
        (
            EmirOp::Branch {
                condition: left_condition,
                args: left_args,
                then_body: left_then,
                else_body: left_else,
            },
            EmirOp::Branch {
                condition: right_condition,
                args: right_args,
                then_body: right_then,
                else_body: right_else,
            },
        ) => {
            left_condition == right_condition
                && left_args == right_args
                && programs_equal(left_then, right_then)
                && programs_equal(left_else, right_else)
        }
        (
            EmirOp::Iterate {
                body: left_body,
                stop: left_stop,
                count: left_count,
                init: left_init,
                args: left_args,
            },
            EmirOp::Iterate {
                body: right_body,
                stop: right_stop,
                count: right_count,
                init: right_init,
                args: right_args,
            },
        ) => {
            left_count == right_count
                && left_init == right_init
                && left_args == right_args
                && programs_equal(left_body, right_body)
                && match (left_stop, right_stop) {
                    (None, None) => true,
                    (Some(left_stop), Some(right_stop)) => programs_equal(left_stop, right_stop),
                    _ => false,
                }
        }
        (left, right) => left == right,
    }
}

fn lower_int_primitive_recur(name: &str, param: &str, body: &Expr) -> Option<EmirProgram> {
    let ExprKind::If {
        condition,
        then_value,
        else_value,
    } = &body.kind
    else {
        return None;
    };
    if contains_self_call(then_value, name) || !is_zero_test(condition, param) {
        return None;
    }
    let (op, coeff) = plus_self_pred(else_value, name, param)?;
    let mut step = Lowerer {
        inputs: vec!["__index".into(), "__acc".into(), param.to_string()],
        locals: BTreeMap::new(),
        siblings: std::rc::Rc::default(),
        entry_siblings: BTreeMap::new(),
        objects: BTreeMap::new(),
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name: None,
        self_result: String::new(),
        arrow_names: BTreeSet::new(),
        module_stamps: BTreeMap::new(),
        type_vars: Vec::new(),
        hints: std::rc::Rc::default(),
    };
    let acc = step.push(EmirOp::LoadInput(1));
    let coeff = if contains_path(coeff, param) {
        let shift = dummy_emir_expr(ExprKind::Binary {
            op: BinaryOp::Sub,
            left: Box::new(path_emir(param)),
            right: Box::new(path_emir("__index")),
        });
        step.expr(&replace_path(coeff, param, &shift)).ok()?
    } else {
        step.expr(coeff).ok()?
    };
    let updated = step.push(match op {
        BinaryOp::Add => EmirOp::F64Add(acc, coeff),
        BinaryOp::Sub => EmirOp::F64Sub(acc, coeff),
        BinaryOp::Mul => EmirOp::F64Mul(acc, coeff),
        _ => return None,
    });
    let body = step.finish(updated);
    let mut outer = Lowerer {
        inputs: vec![param.to_string()],
        locals: BTreeMap::new(),
        siblings: std::rc::Rc::default(),
        entry_siblings: BTreeMap::new(),
        objects: BTreeMap::new(),
        ops: Vec::new(),
        obligations: Vec::new(),
        self_name: None,
        self_result: String::new(),
        arrow_names: BTreeSet::new(),
        module_stamps: BTreeMap::new(),
        type_vars: Vec::new(),
        hints: std::rc::Rc::default(),
    };
    let count = outer.push(EmirOp::LoadInput(0));
    let init = outer.expr(then_value).ok()?;
    let param_value = outer.push(EmirOp::LoadInput(0));
    let result = outer.push(EmirOp::Iterate {
        count,
        init,
        args: vec![param_value],
        stop: None,
        body,
    });
    Some(outer.finish(result))
}

fn dummy_emir_expr(kind: ExprKind) -> Expr {
    Expr {
        kind,
        source: Span::default(),
    }
}

fn path_emir(name: &str) -> Expr {
    dummy_emir_expr(ExprKind::Path {
        segments: vec![name.into()],
        generics: None,
    })
}

fn contains_path(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Path { segments, .. } => segments.join(".") == name,
        ExprKind::Binary { left, right, .. } => {
            contains_path(left, name) || contains_path(right, name)
        }
        ExprKind::Unary { value, .. } => contains_path(value, name),
        ExprKind::Call { function, args } => {
            contains_path(function, name) || args.iter().any(|arg| contains_path(arg, name))
        }
        _ => false,
    }
}

fn replace_path(expr: &Expr, name: &str, replacement: &Expr) -> Expr {
    if is_path(expr, name) {
        return replacement.clone();
    }
    let kind = match &expr.kind {
        ExprKind::Binary { op, left, right } => ExprKind::Binary {
            op: *op,
            left: Box::new(replace_path(left, name, replacement)),
            right: Box::new(replace_path(right, name, replacement)),
        },
        ExprKind::Unary { op, value } => ExprKind::Unary {
            op: *op,
            value: Box::new(replace_path(value, name, replacement)),
        },
        ExprKind::Call { function, args } => ExprKind::Call {
            function: Box::new(replace_path(function, name, replacement)),
            args: args
                .iter()
                .map(|arg| replace_path(arg, name, replacement))
                .collect(),
        },
        other => other.clone(),
    };
    Expr {
        kind,
        source: expr.source,
    }
}

fn is_path(expr: &Expr, name: &str) -> bool {
    matches!(&expr.kind, ExprKind::Path { segments, .. } if segments.join(".") == name)
}

fn is_int(expr: &Expr, want: i64) -> bool {
    matches!(&expr.kind, ExprKind::Int(text) if text.replace('_', "").parse::<i64>().ok() == Some(want))
}

fn is_zero_test(expr: &Expr, param: &str) -> bool {
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Eq | BinaryOp::Le,
            left,
            right,
        } => {
            (is_path(left, param) && is_int(right, 0)) || (is_path(right, param) && is_int(left, 0))
        }
        _ => false,
    }
}

fn is_pred_arg(expr: &Expr, param: &str) -> bool {
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Sub,
            left,
            right,
        } => is_path(left, param) && is_int(right, 1),
        _ => false,
    }
}

fn is_self_pred(expr: &Expr, name: &str, param: &str) -> bool {
    let ExprKind::Call { function, args } = &expr.kind else {
        return false;
    };
    let ExprKind::Path { segments, .. } = &function.kind else {
        return false;
    };
    segments.join(".") == name
        && args.len() == 1
        && args.first().is_some_and(|arg| is_pred_arg(arg, param))
}

fn plus_self_pred<'a>(expr: &'a Expr, name: &str, param: &str) -> Option<(BinaryOp, &'a Expr)> {
    let ExprKind::Binary { op, left, right } = &expr.kind else {
        return None;
    };
    if !matches!(*op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
        return None;
    }
    if is_self_pred(left, name, param) && !contains_self_call(right, name) {
        return Some((*op, right));
    }
    if is_self_pred(right, name, param) && !contains_self_call(left, name) {
        return Some((*op, left));
    }
    None
}

fn contains_self_call(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Call { function, args } => {
            if let ExprKind::Path { segments, .. } = &function.kind {
                if segments.join(".") == name {
                    return true;
                }
            }
            contains_self_call(function, name)
                || args.iter().any(|arg| contains_self_call(arg, name))
        }
        ExprKind::FunctionAbs { body, domain, .. } => {
            contains_self_call(body, name) || contains_self_call(domain, name)
        }
        ExprKind::Recur { body, ty, .. } => {
            contains_self_call(body, name) || contains_self_call(ty, name)
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            contains_self_call(condition, name)
                || contains_self_call(then_value, name)
                || contains_self_call(else_value, name)
        }
        ExprKind::Binary { left, right, .. } => {
            contains_self_call(left, name) || contains_self_call(right, name)
        }
        ExprKind::Unary { value, .. } => contains_self_call(value, name),
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            items.iter().any(|item| contains_self_call(item, name))
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            subject
                .as_ref()
                .is_some_and(|subject| contains_self_call(subject, name))
                || arms.iter().any(|(cond, value)| {
                    contains_self_call(cond, name) || contains_self_call(value, name)
                })
                || contains_self_call(else_arm, name)
        }
        _ => false,
    }
}

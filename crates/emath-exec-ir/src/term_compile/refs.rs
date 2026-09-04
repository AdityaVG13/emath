//! Reference-term compilation entry points and std reference terms.

use super::*;

/// Compile a quoted cell reference term into generic bytecode.
///
/// `signature` is checked with emath-term's own validator first, then
/// every operator is mapped onto the closed generic vocabulary (strict
/// arithmetic, the builtin registry, the closed vector map/reduce set).
/// The compiled program is optimized with the same passes as any other
/// EMIR program. A pure cell needs NO per-op Rust function in the VM
/// seam: the registry below is data.
pub fn compile_reference(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    guards: Vec<ArgGuard>,
    capability: &str,
) -> Result<CompiledCell, TermCompileError> {
    compile_reference_inner(term, signature, params, guards, None, capability)
}

/// [`compile_reference`] plus a post-body zero-certificate guard: the
/// compiled cell refuses typed with the guard's code when its result
/// vector has a nonzero entry. Cell DATA — the seam enforces it
/// generically, no domain branch.
pub fn compile_reference_guarded(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    guards: Vec<ArgGuard>,
    result_guard: ResultGuard,
    capability: &str,
) -> Result<CompiledCell, TermCompileError> {
    compile_reference_inner(
        term,
        signature,
        params,
        guards,
        Some(result_guard),
        capability,
    )
}

pub(super) fn compile_reference_inner(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    guards: Vec<ArgGuard>,
    result_guard: Option<ResultGuard>,
    capability: &str,
) -> Result<CompiledCell, TermCompileError> {
    // emath-term's structural validation (unknown symbols, arity).
    signature.validate(term).map_err(|error| match error {
        TermError::UnknownSymbol(symbol) => TermCompileError::UnknownSymbol { symbol: symbol.0 },
        TermError::ArityMismatch {
            symbol,
            expected,
            actual,
        } => TermCompileError::ArityMismatch {
            symbol: symbol.0,
            expected,
            actual,
        },
        TermError::ConflictingArity {
            symbol,
            first,
            second,
        } => TermCompileError::ConflictingArity {
            symbol: symbol.0,
            first,
            second,
        },
    })?;
    // Guards must reference declared arguments.
    for guard in &guards {
        let index = match guard {
            ArgGuard::NonEmpty(index) | ArgGuard::AllFinite(index) => *index,
        };
        if index >= params.len() {
            return Err(TermCompileError::MalformedContract {
                detail: format!(
                    "guard references argument {index} outside the {} declared param(s)",
                    params.len()
                ),
            });
        }
    }
    let input_count =
        u16::try_from(params.len()).map_err(|_| TermCompileError::MalformedContract {
            detail: "param count exceeds u16::MAX".to_string(),
        })?;
    let mut compiler = Compiler {
        ops: Vec::new(),
        params: params.to_vec(),
    };
    let (result, _shape) = compiler.compile_term(term)?;
    let mut program = EmirProgram {
        ops: compiler.ops,
        result,
        input_count,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    optimize::optimize_program(&mut program);
    Ok(CompiledCell {
        capability: capability.to_string(),
        params: params.to_vec(),
        guards,
        result_guard,
        program,
    })
}

/// The `std.tensor.softmax` reference formula of record, as a quoted
/// term: `exp(sub(x, vmax(x)))` normalized by `sum(exp(sub(x, vmax(x))))`
/// — the stable-max form (shift invariance is the cell's declared law,
/// and the shift keeps strict-f64 exp finite for large logits).
pub(super) fn softmax_reference_term() -> (Term, Signature) {
    let x = || Term::Variable(VariableId("x".into()));
    let shifted = || Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![
            x(),
            Term::Apply {
                operator: SymbolId("vmax".into()),
                arguments: vec![x()],
            },
        ],
    };
    let exps = || Term::Apply {
        operator: SymbolId("exp".into()),
        arguments: vec![shifted()],
    };
    let term = Term::Apply {
        operator: SymbolId("div".into()),
        arguments: vec![
            exps(),
            Term::Apply {
                operator: SymbolId("sum".into()),
                arguments: vec![exps()],
            },
        ],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [
        ("exp", 1usize),
        ("sub", 2),
        ("div", 2),
        ("sum", 1),
        ("vmax", 1),
    ] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("softmax formula signature is conflict-free");
    }
    (term, signature)
}

pub(super) fn compile_std_softmax() -> Result<CompiledCell, TermCompileError> {
    let (term, signature) = softmax_reference_term();
    compile_reference(
        &term,
        &signature,
        &[("x".to_string(), ParamShape::Vector)],
        vec![ArgGuard::NonEmpty(0), ArgGuard::AllFinite(0)],
        "std.tensor.softmax",
    )
}

/// A single-argument scalar cell term: `<op>(x)` (the cohort's unary
/// shapes use the same closed vocabulary).
pub(super) fn scalar_unary_term(op: &str) -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(op.into()), 1)
        .expect("single-op signature is conflict-free");
    let term = Term::Apply {
        operator: SymbolId(op.into()),
        arguments: vec![Term::Variable(VariableId("x".into()))],
    };
    (term, signature)
}

/// A two-argument scalar cell term: `<op>(x, y)`.
pub(super) fn scalar_binary_term(op: &str) -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(op.into()), 2)
        .expect("two-arg signature is conflict-free");
    let term = Term::Apply {
        operator: SymbolId(op.into()),
        arguments: vec![
            Term::Variable(VariableId("x".into())),
            Term::Variable(VariableId("y".into())),
        ],
    };
    (term, signature)
}

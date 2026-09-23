use super::{Term, ParamShape, CompiledCell, EmirValue, DomainObligation, EmirOp, FeatureCapsule, BTreeMap, FeatureId, LanguageImageError, BTreeSet, EmirProgram, FromStr, Signature, SymbolId, TermError, optimize, Span};

/// Image partition carrying the compiled reference programs. The page is
/// DATA (canonical text), never generated Rust source, and it is the
/// loaded-reference source of truth for consumers.
pub(super) const REFERENCE_PARTITION: &str = "language.reference";
pub(super) const REFERENCE_MODE_SLOT: &str = "reference";
pub(super) const REFERENCE_MODE_AUTHORED: &str = "authored";
pub(super) const REFERENCE_PARAMS_SLOT: &str = "reference_params";
pub(super) const REFERENCE_SIGNATURE_SLOT: &str = "reference_signature";
pub(super) const REFERENCE_BODY_SLOT: &str = "reference_body";
pub(super) const REFERENCE_NONE_PAGE: &str = "# none\n";

/// One derived reference program: the authored canonical term it was
/// compiled from, the declared parameters with shapes, and the compiled
/// cell. Kept together so encode/decode/recompile stay one source of truth.
#[derive(Clone, Debug)]
pub(super) struct ReferenceEntry {
    pub(super) term: Term,
    pub(super) defaults: Vec<Term>,
    pub(super) params: Vec<(String, ParamShape)>,
    pub(super) cell: CompiledCell,
}

/// The closed machine-neutral scalar reference vocabulary: symbol tokens
/// mapped onto existing generic EMIR operations. Tokens name operators,
/// never features; extending the set is a schema-recorded decision, not a
/// feature-name branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReferenceOperator {
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    Not,
    IsFinite,
    #[allow(dead_code)]
    Unary(crate::BuiltinId),
    #[allow(dead_code)]
    Binary(crate::BuiltinId),
    #[allow(dead_code)]
    ToInt,
    #[allow(dead_code)]
    IntegerQuotient,
    SameBits,
    #[allow(dead_code)]
    MatrixRows,
    #[allow(dead_code)]
    MatrixCols,
    #[allow(dead_code)]
    MatrixAt,
    #[allow(dead_code)]
    MatrixPack,
    #[allow(dead_code)]
    TensorShape,
    #[allow(dead_code)]
    TensorPack,
    #[allow(dead_code)]
    DenseIndex,
    #[allow(dead_code)]
    Infinity,
    #[allow(dead_code)]
    Exp2,
    #[allow(dead_code)]
    PowF,
    #[allow(dead_code)]
    PowI,
    TextTrim,
    TextLength,
    IndexText,
    TextByte,
    FormatScientific,
    ParseF64,
    SortTotal,
}

impl ReferenceOperator {
    pub(super) fn resolve(symbol: &str, arity: usize) -> Option<Self> {
        match (symbol, arity) {
            ("add", 2) => Some(Self::Add),
            ("sub", 2) => Some(Self::Sub),
            ("mul", 2) => Some(Self::Mul),
            ("div", 2) => Some(Self::Div),
            ("neg", 1) => Some(Self::Neg),
            ("lt", 2) => Some(Self::Lt),
            ("le", 2) => Some(Self::Le),
            ("gt", 2) => Some(Self::Gt),
            ("ge", 2) => Some(Self::Ge),
            ("eq", 2) => Some(Self::Eq),
            ("ne", 2) => Some(Self::Ne),
            ("and", 2) => Some(Self::And),
            ("or", 2) => Some(Self::Or),
            ("not", 1) => Some(Self::Not),
            ("is_finite", 1) => Some(Self::IsFinite),
            ("text_trim", 1) => Some(Self::TextTrim),
            ("text_length", 1) => Some(Self::TextLength),
            ("index_text", 1) => Some(Self::IndexText),
            ("text_byte", 2) => Some(Self::TextByte),
            ("format_scientific", 2) => Some(Self::FormatScientific),
            ("parse_f64", 1) => Some(Self::ParseF64),
            ("sort_total", 1) => Some(Self::SortTotal),
            ("same_bits", 2) => Some(Self::SameBits),
            _ => None,
        }
    }

    pub(super) fn lower(self, operands: &[EmirValue], obligations: &mut Vec<DomainObligation>) -> EmirOp {
        match self {
            Self::Add => EmirOp::F64Add(operands[0], operands[1]),
            Self::Sub => EmirOp::F64Sub(operands[0], operands[1]),
            Self::Mul => EmirOp::F64Mul(operands[0], operands[1]),
            Self::Div => {
                obligations.push(DomainObligation::DivisionNonZero);
                EmirOp::F64Div(operands[0], operands[1])
            }
            Self::Neg => EmirOp::Neg(operands[0]),
            Self::Lt => EmirOp::Lt(operands[0], operands[1]),
            Self::Le => EmirOp::Le(operands[0], operands[1]),
            Self::Gt => EmirOp::Gt(operands[0], operands[1]),
            Self::Ge => EmirOp::Ge(operands[0], operands[1]),
            Self::Eq => EmirOp::Eq(operands[0], operands[1]),
            Self::Ne => EmirOp::Ne(operands[0], operands[1]),
            Self::And => EmirOp::And(operands[0], operands[1]),
            Self::Or => EmirOp::Or(operands[0], operands[1]),
            Self::Not => EmirOp::Not(operands[0]),
            Self::IsFinite => EmirOp::IsFinite(operands[0]),
            Self::Unary(builtin) => EmirOp::UnaryBuiltin(builtin, operands[0]),
            Self::Binary(builtin) => EmirOp::BinaryBuiltin(builtin, operands[0], operands[1]),
            Self::ToInt => EmirOp::ToInt(operands[0]),
            Self::IntegerQuotient => EmirOp::IntegerQuotient(operands[0], operands[1]),
            Self::SameBits => EmirOp::SameBits(operands[0], operands[1]),
            Self::MatrixRows => EmirOp::MatrixRows(operands[0]),
            Self::MatrixCols => EmirOp::MatrixCols(operands[0]),
            Self::MatrixAt => EmirOp::MatrixIndex { matrix: operands[0], row: operands[1], col: operands[2] },
            Self::MatrixPack => EmirOp::MatrixPack { rows: operands[0], cols: operands[1], data: operands[2] },
            Self::Infinity => EmirOp::ConstF64(f64::INFINITY.to_bits()),
            Self::Exp2 => EmirOp::F64Exp2(operands[0]),
            Self::PowF => EmirOp::F64Pow(operands[0], operands[1]),
            Self::PowI => EmirOp::F64PowI(operands[0], operands[1]),
            Self::TextTrim => EmirOp::TextTrim(operands[0]),
            Self::TextLength => EmirOp::TextLength(operands[0]),
            Self::IndexText => EmirOp::IndexText(operands[0]),
            Self::TextByte => EmirOp::TextByte(operands[0], operands[1]),
            Self::FormatScientific => EmirOp::FormatScientific(operands[0], operands[1]),
            Self::ParseF64 => EmirOp::ParseF64(operands[0]),
            Self::SortTotal => EmirOp::F64SortTotal(operands[0]),
            Self::TensorShape => EmirOp::TensorShape(operands[0]),
            Self::TensorPack => EmirOp::TensorPack { shape: operands[0], data: operands[1] },
            Self::DenseIndex => EmirOp::DenseIndex { dense: operands[0], index: operands[1] },
        }
    }
}

/// Compiles the authored reference bodies of a capsule set into
/// capability-keyed entries, in deterministic feature order.
pub(super) fn compile_reference_entries(
    capsules: &[FeatureCapsule],
) -> Result<BTreeMap<FeatureId, ReferenceEntry>, LanguageImageError> {
    let mut entries = BTreeMap::new();
    for capsule in capsules {
        if let Some(entry) = reference_entry_for_capsule(capsule)? {
            if entries.insert(capsule.feature_id.clone(), entry).is_some() {
                return Err(LanguageImageError::DuplicateFeature(
                    capsule.feature_id.clone(),
                ));
            }
        }
    }
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for feature in entries.keys() {
        validate_reference_calls(feature, &entries, &mut active, &mut complete)?;
    }
    Ok(entries)
}

pub(super) fn validate_reference_calls(
    feature: &FeatureId,
    entries: &BTreeMap<FeatureId, ReferenceEntry>,
    active: &mut BTreeSet<FeatureId>,
    complete: &mut BTreeSet<FeatureId>,
) -> Result<(), LanguageImageError> {
    if complete.contains(feature) { return Ok(()); }
    if !active.insert(feature.clone()) {
        return Err(LanguageImageError::InvalidReferenceBody { feature: feature.clone(), detail: "cyclic authored calls require explicit bounded iteration".into() });
    }
    pub(super) fn dependencies(program: &EmirProgram, out: &mut BTreeSet<FeatureId>) {
        for (op, _) in &program.ops {
            match op {
                EmirOp::ApplyCapability { capability, .. } => { if let Ok(feature) = FeatureId::from_str(capability) { out.insert(feature); } }
                EmirOp::Branch { then_body, else_body, .. } => { dependencies(then_body, out); dependencies(else_body, out); }
                EmirOp::Iterate { body, stop, .. } => { if let Some(stop) = stop { dependencies(stop, out); } dependencies(body, out); }
                EmirOp::Fold { body, .. } | EmirOp::Collect { body, .. }
            | EmirOp::CallFrame { body, .. } | EmirOp::ProgramLiteral { body, .. } => dependencies(body, out),
                _ => {}
            }
        }
    }
    if let Some(entry) = entries.get(feature) {
        let mut called = BTreeSet::new();
        dependencies(&entry.cell.program, &mut called);
        for default in &entry.cell.defaults { dependencies(default, &mut called); }
        for callee in called {
            if entries.contains_key(&callee) { validate_reference_calls(&callee, entries, active, complete)?; }
        }
    }
    active.remove(feature);
    complete.insert(feature.clone());
    Ok(())
}

/// Derives one capsule's reference entry, or `None` when the capsule
/// declares no executable reference body. All three body slots are
/// required together, and their presence requires the `authored` mode.
pub(super) fn reference_entry_for_capsule(
    capsule: &FeatureCapsule,
) -> Result<Option<ReferenceEntry>, LanguageImageError> {
    let params_slot = reference_slot(capsule, REFERENCE_PARAMS_SLOT);
    let signature_slot = reference_slot(capsule, REFERENCE_SIGNATURE_SLOT);
    let body_slot = reference_slot(capsule, REFERENCE_BODY_SLOT);
    let defaults_slot = reference_slot(capsule, "reference_defaults");
    if params_slot.is_none()
        && signature_slot.is_none()
        && body_slot.is_none()
        && defaults_slot.is_none()
    {
        return Ok(None);
    }
    let refuse = |detail: String| {
        Err(LanguageImageError::InvalidReferenceBody {
            feature: capsule.feature_id.clone(),
            detail,
        })
    };
    let (Some(params_text), Some(body_text)) = (params_slot, body_slot) else {
        return refuse(
            "executable reference bodies carry reference_params and reference_body"
                .to_string(),
        );
    };
    match reference_slot(capsule, REFERENCE_MODE_SLOT) {
        Some(REFERENCE_MODE_AUTHORED) => {}
        other => {
            return refuse(format!(
                "an executable reference body requires reference mode \
                 `{REFERENCE_MODE_AUTHORED}`, found `{}`",
                other.unwrap_or("missing"),
            ));
        }
    }
    let mut params = Vec::new();
    for token in params_text.split(',') {
        let name = token.trim();
        if name.is_empty() || params.iter().any(|(declared, _)| declared == name) {
            return refuse(format!(
                "reference_params `{params_text}` names no parameter or repeats one"
            ));
        }
        params.push((name.to_string(), ParamShape::Scalar));
    }
    let term = Term::parse_body(body_text).map_err(|error| {
        LanguageImageError::InvalidReferenceBody {
            feature: capsule.feature_id.clone(),
            detail: format!("reference_body is not an emath-term: {error:?}"),
        }
    })?;
    let signature = match signature_slot {
        Some(signature_text) => {
            let mut signature = Signature::default();
            for pair in signature_text.split(',') {
                let Some((symbol, arity)) = pair.trim().rsplit_once('=') else {
                    return refuse(format!(
                        "reference_signature `{signature_text}` must declare `symbol=arity` pairs"
                    ));
                };
                let Ok(arity) = arity.trim().parse::<usize>() else {
                    return refuse(format!(
                        "reference_signature `{signature_text}` declares a non-numeric arity"
                    ));
                };
                if let Err(error) = signature.insert(SymbolId(symbol.trim().to_string()), arity) {
                    return refuse(format!(
                        "reference_signature `{signature_text}` conflicts: {error:?}"
                    ));
                }
            }
            signature
        }
        None => term.inferred_signature().map_err(|error| {
            LanguageImageError::InvalidReferenceBody {
                feature: capsule.feature_id.clone(),
                detail: format!("could not infer reference signature: {error:?}"),
            }
        })?,
    };
    let Some(semantics) = reference_slot(capsule, "semantics") else {
        return refuse("an executable reference body requires the semantics slot".to_string());
    };
    let shapes = declared_input_shapes(semantics);
    if shapes.len() != params.len() {
        return refuse(format!(
            "semantics declares {} input(s) but the reference body names {} parameter(s)",
            shapes.len(),
            params.len(),
        ));
    }
    for ((_, shape), declared) in params.iter_mut().zip(shapes) {
        *shape = declared;
    }
    let defaults = defaults_slot
        .map(|text| {
            let mut depth = 0usize;
            let mut escaped = false;
            text.split(|ch| {
                if escaped {
                    escaped = false;
                    return false;
                }
                match ch {
                    '\\' => escaped = true,
                    '(' => depth += 1,
                    ')' => depth = depth.saturating_sub(1),
                    ';' if depth == 0 => return true,
                    _ => {}
                }
                false
            })
            .map(|term| Term::parse_canonical(term.trim()))
            .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|error| LanguageImageError::InvalidReferenceBody {
            feature: capsule.feature_id.clone(),
            detail: format!("invalid reference default: {error:?}"),
        })?
        .unwrap_or_default();
    if defaults_slot.is_some() {
        let arity = semantics
            .split(';')
            .find_map(|part| part.trim().strip_prefix("arity="))
            .and_then(crate::native_kernel::parse_kernel_arity);
        let range = match arity {
            Some(crate::native_kernel::KernelArity::Exact(count)) => Some((count, count)),
            Some(crate::native_kernel::KernelArity::Bounded { min, max }) => Some((min, max)),
            None => None,
        };
        if params
            .len()
            .checked_sub(defaults.len())
            .map(|min| (min, params.len()))
            != range
        {
            return refuse("reference defaults do not match the declared arity range".into());
        }
    }
    compile_reference_with_defaults(
        &term,
        &defaults,
        &signature,
        &params,
        capsule.feature_id.as_str(),
    )
    .map(|cell| {
        Some(ReferenceEntry {
            term,
            defaults,
            params,
            cell,
        })
    })
    .map_err(|detail| LanguageImageError::InvalidReferenceBody {
        feature: capsule.feature_id.clone(),
        detail,
    })
}

pub(super) fn reference_slot<'a>(capsule: &'a FeatureCapsule, name: &str) -> Option<&'a str> {
    match capsule.slots.get(name) {
        Some(emath_ir::CapsuleSlot::Value(value)) => Some(value.as_str()),
        _ => None,
    }
}

/// Shapes declared by the semantics slot's `inputs=` field, in argument
/// order. The mapping is generic over the declared type tokens —
/// `Vector<Float64>` and `Matrix<Float64>` carry their carriers, every
/// other token is a scalar.
pub(super) fn declared_input_shapes(semantics: &str) -> Vec<ParamShape> {
    semantics
        .split(';')
        .find_map(|field| field.trim().strip_prefix("inputs="))
        .map(|inputs| {
            inputs
                .split(',')
                .map(|token| {
                    let token = token.trim();
                    if token == "Rat" {
                        ParamShape::Rational
                    } else if token.starts_with("Vector") {
                        ParamShape::Vector
                    } else if token.starts_with("Matrix") {
                        ParamShape::Matrix
                    } else {
                        ParamShape::Scalar
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Compiles one validated reference term into a `CompiledCell` over the
/// closed scalar vocabulary. The refusal detail is caller-shaped: capsule
/// derivation and image recompilation wrap it in their own error types.
pub(super) fn compile_reference_term(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    capability: &str,
) -> Result<CompiledCell, String> {
    signature.validate(term).map_err(|error| match error {
        TermError::UnknownSymbol(symbol) => {
            format!("reference term uses undeclared symbol `{}`", symbol.0)
        }
        TermError::ArityMismatch {
            symbol,
            expected,
            actual,
        } => format!(
            "reference operator `{}` applied to {actual} argument(s), the capsule declares {expected}",
            symbol.0
        ),
        TermError::ConflictingArity {
            symbol,
            first,
            second,
        } => format!(
            "reference symbol `{}` is declared with conflicting arities {first} and {second}",
            symbol.0
        ),
    })?;
    let mut compiler = ReferenceCompiler {
        next_register: 0,
        ops: Vec::new(),
        obligations: Vec::new(),
        locals: BTreeMap::new(),
        params,
    };
    let result = compiler.emit(term)?;
    let mut program = EmirProgram {
        ops: compiler.ops,
        result,
        input_count: u16::try_from(params.len())
            .map_err(|_| "reference cells exceed u16::MAX parameters".to_string())?,
        state_count: 0,
        domain_obligations: compiler.obligations,
    };
    optimize::optimize_program(&mut program);
    Ok(CompiledCell {
        capability: capability.to_string(),
        params: params.to_vec(),
        defaults: Vec::new(),
        guards: Vec::new(),
        result_guard: None,
        program,
    })
}

pub(super) fn compile_reference_with_defaults(
    term: &Term, defaults: &[Term], signature: &Signature,
    params: &[(String, ParamShape)], capability: &str,
) -> Result<CompiledCell, String> {
    let first = params.len().checked_sub(defaults.len())
        .ok_or_else(|| "reference defaults exceed parameter count".to_string())?;
    let mut cell = compile_reference_term(term, signature, params, capability)?;
    for (index, default) in defaults.iter().enumerate() {
        cell.defaults.push(compile_reference_term(default, signature, &params[..first + index], capability)?.program);
    }
    Ok(cell)
}

/// Lowers a validated canonical term onto the closed scalar vocabulary.
/// Registers are op indices; every op is stamped with a default span
/// because the authored source is capsule data, not a positioned file.
pub(super) struct ReferenceCompiler<'a> {
    next_register: u32,
    ops: Vec<(EmirOp, Span)>,
    obligations: Vec<DomainObligation>,
    params: &'a [(String, ParamShape)],
    locals: BTreeMap<String, EmirValue>,
}

impl ReferenceCompiler<'_> {
    pub(super) fn push(&mut self, op: EmirOp) -> EmirValue {
        let register = EmirValue(self.next_register);
        self.next_register += 1;
        self.ops.push((op, Span::default()));
        register
    }

    pub(super) fn capture(&mut self) -> Vec<(String, EmirValue)> {
        for (index, (name, _)) in self.params.iter().enumerate() {
            if !self.locals.contains_key(name) {
                let value = self.push(EmirOp::LoadInput(index as u16));
                self.locals.insert(name.clone(), value);
            }
        }
        self.locals.iter().map(|(name, value)| (name.clone(), *value)).collect()
    }

    pub(super) fn nested(term: &Term, names: Vec<String>) -> Result<EmirProgram, String> {
        let input_count = u16::try_from(names.len()).map_err(|_| "too many captured arguments".to_string())?;
        let params = names.into_iter().map(|name| (name, ParamShape::Scalar)).collect::<Vec<_>>();
        let mut compiler = ReferenceCompiler { next_register: 0, ops: Vec::new(), obligations: Vec::new(), params: &params, locals: BTreeMap::new() };
        let result = compiler.emit(term)?;
        Ok(EmirProgram { ops: compiler.ops, result, input_count, state_count: 0, domain_obligations: compiler.obligations })
    }

    pub(super) fn emit_control(&mut self, symbol: &str, arguments: &[Term]) -> Result<Option<EmirValue>, String> {
        let literal = |term: &Term| match term {
            Term::Constant(symbol) => Ok(symbol.0.clone()),
            _ => Err("record fields and refusal codes require literal symbols".to_string()),
        };
        let op = match (symbol, arguments) {
            ("let", [Term::Variable(name), init, body]) => {
                let init = self.emit(init)?;
                let previous = self.locals.insert(name.0.clone(), init);
                let result = self.emit(body);
                if let Some(previous) = previous { self.locals.insert(name.0.clone(), previous); } else { self.locals.remove(&name.0); }
                return result.map(Some);
            }
            ("if", [condition, then_body, else_body]) => {
                let condition = self.emit(condition)?;
                let captured = self.capture();
                let names = captured.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
                EmirOp::Branch { condition, args: captured.into_iter().map(|(_, value)| value).collect(), then_body: Self::nested(then_body, names.clone())?, else_body: Self::nested(else_body, names)? }
            }
            ("collect", [Term::Variable(index), count, body]) => {
                let count = self.emit(count)?;
                let captured = self.capture().into_iter().filter(|(name, _)| name != &index.0).collect::<Vec<_>>();
                let mut names = vec![index.0.clone()];
                names.extend(captured.iter().map(|(name, _)| name.clone()));
                EmirOp::Collect { count, args: captured.into_iter().map(|(_, value)| value).collect(), body: Self::nested(body, names)? }
            }
            (symbol, [Term::Variable(index), Term::Variable(state), count, init, rest @ ..]) if matches!(symbol, "iterate" | "iterate_until") => {
                let (body, stop) = match (symbol, rest) {
                    ("iterate", [body]) => (body, None),
                    ("iterate_until", [stop, body]) => (body, Some(stop)),
                    _ => return Err("bounded iteration has the wrong argument count".into()),
                };
                if index == state { return Err("iteration index and accumulator require distinct names".into()); }
                let count = self.emit(count)?;
                let init = self.emit(init)?;
                let captured = self.capture().into_iter().filter(|(name, _)| name != &index.0 && name != &state.0).collect::<Vec<_>>();
                let mut names = vec![index.0.clone(), state.0.clone()];
                names.extend(captured.iter().map(|(name, _)| name.clone()));
                let body = Self::nested(body, names.clone())?;
                let stop = stop.map(|stop| Self::nested(stop, names)).transpose()?;
                EmirOp::Iterate { count, init, args: captured.into_iter().map(|(_, value)| value).collect(), stop, body }
            }
            ("get", [record, field]) => EmirOp::RecordField { record: self.emit(record)?, field: literal(field)? },
            ("refuse", [detail]) => {
                let detail = literal(detail)?;
                EmirOp::Refuse(detail.strip_prefix("text:").unwrap_or(&detail).to_string())
            }
            ("refuse_value", [value]) => EmirOp::RefuseValue(self.emit(value)?),
            ("format_text", [template, arguments @ ..]) => {
                let template = literal(template)?;
                EmirOp::FormatText {
                    template: template.strip_prefix("text:").unwrap_or(&template).to_string(),
                    arguments: arguments.iter().map(|value| self.emit(value)).collect::<Result<Vec<_>, _>>()?,
                }
            }
            ("call_program", [program, inputs]) => EmirOp::CallProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("call_scalar_program", [program, inputs]) => EmirOp::CallScalarProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("call_real_program", [program, inputs]) => EmirOp::CallRealProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("try_call_real_program", [program, inputs]) => EmirOp::TryCallRealProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("result_is_ok", [value]) => EmirOp::ResultIsOk(self.emit(value)?),
            ("result_unwrap_or", [value, fallback]) => EmirOp::ResultUnwrapOr(self.emit(value)?, self.emit(fallback)?),
            ("vector", [value]) => EmirOp::ToF64Vector(self.emit(value)?),
            ("length", [value]) => EmirOp::VectorLength(self.emit(value)?),
            ("index", [value, index]) => EmirOp::VectorIndex { vector: self.emit(value)?, index: self.emit(index)? },
            ("list", elements) => EmirOp::ListCreate(elements.iter().map(|element| self.emit(element)).collect::<Result<Vec<_>, _>>()?),
            (symbol, values) if symbol.starts_with("record:") => {
                let mut parts = symbol[7..].split(':');
                let type_name = parts.next().unwrap_or_default().to_string();
                let fields = parts.collect::<Vec<_>>();
                if type_name.is_empty() || fields.len() != values.len() { return Err("record constructor requires a type and one field name per value".into()); }
                let mut seen = BTreeSet::new();
                let mut members = Vec::with_capacity(values.len());
                for (field, value) in fields.into_iter().zip(values) {
                    if field.is_empty() || !seen.insert(field) { return Err("record constructor has an empty or repeated field".into()); }
                    members.push((field.to_string(), self.emit(value)?));
                }
                EmirOp::RecordCreate { type_name, fields: members }
            }
            (symbol, values) if FeatureId::from_str(symbol).is_ok() => EmirOp::ApplyCapability { capability: symbol.to_string(), class: crate::CellClass::Pure, args: values.iter().map(|value| self.emit(value)).collect::<Result<Vec<_>, _>>()? },
            _ => return Ok(None),
        };
        Ok(Some(self.push(op)))
    }

    pub(super) fn emit(&mut self, term: &Term) -> Result<EmirValue, String> {
        match term {
            Term::Variable(variable) => {
                if let Some(value) = self.locals.get(&variable.0) { return Ok(*value); }
                let index = self
                    .params
                    .iter()
                    .position(|(name, _)| name == &variable.0)
                    .ok_or_else(|| {
                        format!(
                            "reference term uses variable `{}` outside the declared parameter list",
                            variable.0
                        )
                    })?;
                let index = u16::try_from(index)
                    .map_err(|_| "reference cells exceed u16::MAX parameters".to_string())?;
                let value = self.push(EmirOp::LoadInput(index));
                self.locals.insert(variable.0.clone(), value);
                Ok(value)
            }
            Term::Constant(symbol) => {
                let op = match symbol.0.as_str() {
                    "true" => EmirOp::ConstBool(true),
                    "false" => EmirOp::ConstBool(false),
                    text if text.starts_with("text:") => EmirOp::ConstText(text[5..].to_string()),
                    text if text.starts_with("i64:") => {
                        EmirOp::ConstI64(text[4..].parse().map_err(|_| {
                            format!("reference integer literal `{text}` is outside i64")
                        })?)
                    }
                    text => {
                        let value = text
                            .parse::<f64>()
                            .ok()
                            .filter(|value| value.is_finite())
                            .ok_or_else(|| {
                                format!(
                                    "reference constant `{text}` is not a finite binary64 literal"
                                )
                            })?;
                        EmirOp::ConstF64(value.to_bits())
                    }
                };
                Ok(self.push(op))
            }
            Term::Apply {
                operator,
                arguments,
            } => {
                if let Some(value) = self.emit_control(&operator.0, arguments)? { return Ok(value); }
                let resolved = ReferenceOperator::resolve(&operator.0, arguments.len())
                    .ok_or_else(|| {
                        format!(
                            "reference operator `{}` with {} argument(s) is outside the \
                             closed machine-neutral scalar vocabulary",
                            operator.0,
                            arguments.len()
                        )
                    })?;
                let mut operands = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    operands.push(self.emit(argument)?);
                }
                let op = resolved.lower(&operands, &mut self.obligations);
                Ok(self.push(op))
            }
        }
    }
}


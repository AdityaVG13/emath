//! Payload encoding/decoding for symbols and semantics.

use super::*;

pub(super) fn operational_payloads(
    change: &SemanticChange,
) -> Result<(Option<&str>, &str), DeltaError> {
    match change.description.split_once(PATCH_SEPARATOR) {
        Some((prior, next)) => Ok((Some(prior), next)),
        None => match change.kind {
            SemanticVariableKind::Constructor
            | SemanticVariableKind::Law
            | SemanticVariableKind::Effect => {
                let prior = change
                    .symbol
                    .as_ref()
                    .ok_or_else(|| DeltaError::MissingTarget {
                        kind: change.kind,
                        target: String::new(),
                    })?;
                Ok((Some(prior.0.as_str()), change.description.as_str()))
            }
            _ => Ok((None, change.description.as_str())),
        },
    }
}

pub(super) fn apply_carrier(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    name: &str,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let carrier = world
        .carriers
        .iter_mut()
        .find(|carrier| carrier.name == name)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: name.to_string(),
        })?;
    check_prior(kind, name, expected, &carrier.type_expression)?;
    carrier.type_expression = write.to_string();
    Ok(())
}

pub(super) fn apply_symbol(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    target: &SymbolId,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let symbol = world
        .symbols
        .iter_mut()
        .find(|symbol| symbol.id == *target)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        })?;
    check_prior(kind, &target.0, expected, &encode_symbol_payload(symbol))?;
    write_symbol_payload(symbol, kind, write)
}

pub(super) fn apply_signature(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    target: &SymbolId,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let actual = world
        .signature
        .arity(target)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        })?;
    check_prior(kind, &target.0, expected, &actual.to_string())?;
    let arity = parse_arity(kind, write)?;
    world.signature = with_arity(&world.signature, target, arity)?;
    Ok(())
}

pub(super) fn apply_operator(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    target: &SymbolId,
    expected: Option<&str>,
    write: &str,
    constant_only: bool,
) -> Result<(), DeltaError> {
    if constant_only && !is_constant_symbol(world, target) {
        return Err(DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        });
    }
    let operator = world
        .operators
        .iter_mut()
        .find(|operator| operator.symbol == *target)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        })?;
    check_prior(
        kind,
        &target.0,
        expected,
        &encode_semantics(&operator.semantics),
    )?;
    operator.semantics = decode_semantics(kind, write)?;
    Ok(())
}

pub(super) fn replace_list_item(
    items: &mut [String],
    kind: SemanticVariableKind,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let from = expected.ok_or(DeltaError::NotReversible { kind })?;
    let index =
        items
            .iter()
            .position(|item| item == from)
            .ok_or_else(|| DeltaError::MissingTarget {
                kind,
                target: from.to_string(),
            })?;
    items[index] = write.to_string();
    Ok(())
}

pub(super) fn check_prior(
    kind: SemanticVariableKind,
    target: &str,
    expected: Option<&str>,
    actual: &str,
) -> Result<(), DeltaError> {
    if let Some(expected) = expected {
        if expected != actual {
            return Err(DeltaError::PriorMismatch {
                kind,
                target: target.to_string(),
                expected: expected.to_string(),
                actual: actual.to_string(),
            });
        }
    }
    Ok(())
}

pub(super) fn with_arity(
    signature: &Signature,
    symbol: &SymbolId,
    arity: usize,
) -> Result<Signature, DeltaError> {
    let mut next = Signature::default();
    for (id, existing) in signature.iter() {
        let value = if id == symbol { arity } else { *existing };
        next.insert(id.clone(), value)
            .map_err(|err| DeltaError::MalformedPatch {
                kind: SemanticVariableKind::Signature,
                reason: format!("{err:?}"),
            })?;
    }
    Ok(next)
}

pub(super) fn parse_arity(kind: SemanticVariableKind, payload: &str) -> Result<usize, DeltaError> {
    payload.parse().map_err(|_| DeltaError::MalformedPatch {
        kind,
        reason: format!("arity is not a usize: {payload}"),
    })
}

pub(super) fn is_constant_symbol(world: &WorldIr, symbol: &SymbolId) -> bool {
    world
        .symbols
        .iter()
        .any(|item| item.id == *symbol && item.fixity == Fixity::Constant)
        || world.signature.arity(symbol) == Some(0)
}

pub(super) fn encode_symbol_payload(symbol: &SymbolDef) -> String {
    format!(
        "{}:{}:{}",
        fixity_name(symbol.fixity),
        symbol
            .precedence
            .map_or_else(|| "-".to_string(), |p| p.to_string()),
        symbol.type_scheme
    )
}

pub(super) fn write_symbol_payload(
    symbol: &mut SymbolDef,
    kind: SemanticVariableKind,
    payload: &str,
) -> Result<(), DeltaError> {
    let mut parts = payload.splitn(3, ':');
    let Some(fixity_part) = parts.next() else {
        return Err(DeltaError::MalformedPatch {
            kind,
            reason: "empty symbol payload".to_string(),
        });
    };
    match (parts.next(), parts.next()) {
        (Some(precedence_part), Some(scheme)) => {
            symbol.fixity = parse_fixity(kind, fixity_part)?;
            symbol.precedence = if precedence_part == "-" {
                None
            } else {
                Some(
                    precedence_part
                        .parse()
                        .map_err(|_| DeltaError::MalformedPatch {
                            kind,
                            reason: format!("precedence is not a u16: {precedence_part}"),
                        })?,
                )
            };
            symbol.type_scheme = scheme.to_string();
        }
        _ => symbol.type_scheme = payload.to_string(),
    }
    Ok(())
}

pub(super) fn fixity_name(fixity: Fixity) -> &'static str {
    match fixity {
        Fixity::Constant => "constant",
        Fixity::Prefix => "prefix",
        Fixity::Infix => "infix",
        Fixity::Postfix => "postfix",
        Fixity::Function => "function",
    }
}

pub(super) fn parse_fixity(kind: SemanticVariableKind, name: &str) -> Result<Fixity, DeltaError> {
    match name {
        "constant" => Ok(Fixity::Constant),
        "prefix" => Ok(Fixity::Prefix),
        "infix" => Ok(Fixity::Infix),
        "postfix" => Ok(Fixity::Postfix),
        "function" => Ok(Fixity::Function),
        _ => Err(DeltaError::MalformedPatch {
            kind,
            reason: format!("unknown fixity: {name}"),
        }),
    }
}

pub(super) fn encode_semantics(semantics: &OperatorSemantics) -> String {
    match semantics {
        OperatorSemantics::StructuralConstructor => "structural".to_string(),
        OperatorSemantics::DeclaredExpression(text) => format!("expr:{text}"),
        OperatorSemantics::FiniteTable(rows) => format!("table:{}", rows.join("\u{1e}")),
        OperatorSemantics::ProviderBinding(id) => format!("provider:{id}"),
        OperatorSemantics::Synthesized { program, receipt } => {
            format!("synth:{program}\u{1e}{receipt}")
        }
        OperatorSemantics::Parametric(MeaningHoleId(id)) => format!("hole:{id}"),
    }
}

pub(super) fn decode_semantics(
    kind: SemanticVariableKind,
    payload: &str,
) -> Result<OperatorSemantics, DeltaError> {
    if payload == "structural" {
        return Ok(OperatorSemantics::StructuralConstructor);
    }
    if let Some(text) = payload.strip_prefix("expr:") {
        return Ok(OperatorSemantics::DeclaredExpression(text.to_string()));
    }
    if let Some(rows) = payload.strip_prefix("table:") {
        return Ok(OperatorSemantics::FiniteTable(if rows.is_empty() {
            Vec::new()
        } else {
            rows.split('\u{1e}').map(str::to_string).collect()
        }));
    }
    if let Some(id) = payload.strip_prefix("provider:") {
        return Ok(OperatorSemantics::ProviderBinding(id.to_string()));
    }
    if let Some(rest) = payload.strip_prefix("synth:") {
        let (program, receipt) =
            rest.split_once('\u{1e}')
                .ok_or_else(|| DeltaError::MalformedPatch {
                    kind,
                    reason: "synthesized payload needs program and receipt".to_string(),
                })?;
        return Ok(OperatorSemantics::Synthesized {
            program: program.to_string(),
            receipt: receipt.to_string(),
        });
    }
    if let Some(id) = payload.strip_prefix("hole:") {
        let id = id.parse().map_err(|_| DeltaError::MalformedPatch {
            kind,
            reason: format!("hole id is not a u64: {id}"),
        })?;
        return Ok(OperatorSemantics::Parametric(MeaningHoleId(id)));
    }
    Ok(OperatorSemantics::DeclaredExpression(payload.to_string()))
}

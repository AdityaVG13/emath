use super::*;

/// Canonical page encoding: one stamped entry per capability, in feature
/// order. The printed program is length-prefixed so its embedded newlines
/// never confuse the line-oriented header parse.
pub(super) fn encode_reference_partition(entries: &BTreeMap<FeatureId, ReferenceEntry>) -> String {
    if entries.is_empty() {
        return REFERENCE_NONE_PAGE.to_string();
    }
    let mut page = String::new();
    for (feature, entry) in entries {
        page.push_str("reference ");
        page.push_str(feature.as_str());
        page.push('\n');
        page.push_str("params ");
        for (index, (name, shape)) in entry.params.iter().enumerate() {
            if index > 0 {
                page.push(',');
            }
            page.push_str(name);
            page.push(':');
            page.push_str(shape.as_str());
        }
        page.push('\n');
        page.push_str(&format!("defaults {}\n", entry.defaults.len()));
        for default in &entry.defaults {
            page.push_str("default ");
            page.push_str(&default.canonical());
            page.push('\n');
        }
        page.push_str("term ");
        page.push_str(&entry.term.canonical());
        page.push('\n');
        let program = entry.cell.program.print();
        page.push_str(&format!("program {}\n", program.as_bytes().len()));
        page.push_str(&program);
    }
    page
}

pub(super) fn decode_reference_entries(
    page: &str,
) -> Result<BTreeMap<FeatureId, ReferenceEntry>, LanguageImageError> {
    let malformed = |detail: String| LanguageImageError::ReferencePartitionMalformed(detail);
    if page == REFERENCE_NONE_PAGE {
        return Ok(BTreeMap::new());
    }
    let mut entries = BTreeMap::new();
    let mut rest = page;
    loop {
        let Some(line) = next_line(&mut rest) else {
            break;
        };
        let Some(feature_text) = line.strip_prefix("reference ") else {
            return Err(malformed(format!(
                "expected `reference <feature>` header, found `{line}`"
            )));
        };
        let feature = FeatureId::from_str(feature_text).map_err(|error| {
            malformed(format!(
                "reference key `{feature_text}` is not a feature id: {error}"
            ))
        })?;
        let params_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before params".to_string()))?;
        let Some(params_text) = params_line.strip_prefix("params ") else {
            return Err(malformed(format!(
                "expected `params` line, found `{params_line}`"
            )));
        };
        let params = parse_partition_params(params_text).map_err(malformed)?;
        let defaults_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before defaults".into()))?;
        let count = defaults_line.strip_prefix("defaults ").and_then(|text| text.parse::<usize>().ok())
            .filter(|count| *count <= params.len())
            .ok_or_else(|| malformed("invalid reference default count".into()))?;
        let mut defaults = Vec::with_capacity(count);
        for _ in 0..count {
            let text = next_line(&mut rest).and_then(|line| line.strip_prefix("default "))
                .ok_or_else(|| malformed("reference entry ends before a default term".into()))?;
            defaults.push(Term::parse_canonical(text).map_err(|error| malformed(format!("invalid reference default: {error:?}")))?);
        }
        let term_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before term".to_string()))?;
        let Some(term_text) = term_line.strip_prefix("term ") else {
            return Err(malformed(format!(
                "expected `term` line, found `{term_line}`"
            )));
        };
        let term = Term::parse_canonical(term_text).map_err(|error| {
            malformed(format!(
                "reference term is not canonical emath-term text: {error:?}"
            ))
        })?;
        let program_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before program".to_string()))?;
        let Some(length_text) = program_line.strip_prefix("program ") else {
            return Err(malformed(format!(
                "expected `program` line, found `{program_line}`"
            )));
        };
        let length: usize = length_text
            .parse()
            .map_err(|_| malformed(format!("program length `{length_text}` is not a size")))?;
        if rest.len() < length {
            return Err(malformed(
                "reference program bytes are truncated".to_string(),
            ));
        }
        let (program_text, remainder) = rest.split_at(length);
        rest = remainder;
        let mut signature = Signature::default();
        note_term_arities(&term, &mut signature)
            .map_err(|detail| malformed(format!("reference entry declares {detail}")))?;
        for default in &defaults {
            note_term_arities(default, &mut signature).map_err(|detail| malformed(format!("reference default declares {detail}")))?;
        }
        let cell = compile_reference_with_defaults(&term, &defaults, &signature, &params, feature.as_str()).map_err(
            |detail| {
                malformed(format!(
                    "reference entry `{}` refuses recompilation: {detail}",
                    feature.as_str()
                ))
            },
        )?;
        if cell.program.print() != program_text {
            return Err(LanguageImageError::ReferenceBytecodeMismatch {
                feature: feature.clone(),
            });
        }
        if entries
            .insert(feature, ReferenceEntry { term, defaults, params, cell })
            .is_some()
        {
            return Err(malformed(format!(
                "reference key `{feature_text}` appears twice"
            )));
        }
    }
    Ok(entries)
}

pub(super) fn next_line<'a>(rest: &mut &'a str) -> Option<&'a str> {
    if rest.is_empty() {
        return None;
    }
    match rest.split_once('\n') {
        Some((line, remainder)) => {
            *rest = remainder;
            Some(line)
        }
        None => {
            let line = *rest;
            *rest = "";
            Some(line)
        }
    }
}

pub(super) fn parse_partition_params(text: &str) -> Result<Vec<(String, ParamShape)>, String> {
    text.split(',')
        .map(|token| {
            let (name, shape) = token
                .trim()
                .split_once(':')
                .ok_or_else(|| format!("parameter `{token}` lacks a `name:shape` shape"))?;
            let shape = match shape.trim() {
                "scalar" => ParamShape::Scalar,
                "rational" => ParamShape::Rational,
                "vector" => ParamShape::Vector,
                "matrix" => ParamShape::Matrix,
                other => return Err(format!("unknown parameter shape `{other}`")),
            };
            Ok((name.trim().to_string(), shape))
        })
        .collect()
}

/// Derives the signature a decoded term actually uses, so recompilation
/// needs no capsule context: the page carries the full contract.
pub(super) fn note_term_arities(term: &Term, signature: &mut Signature) -> Result<(), String> {
    match term {
        Term::Variable(_) => Ok(()),
        Term::Constant(symbol) => signature
            .insert(symbol.clone(), 0)
            .map_err(|error| format!("conflicting arities: {error:?}")),
        Term::Apply {
            operator,
            arguments,
        } => {
            signature
                .insert(operator.clone(), arguments.len())
                .map_err(|error| format!("conflicting arities: {error:?}"))?;
            for argument in arguments {
                note_term_arities(argument, signature)?;
            }
            Ok(())
        }
    }
}

/// Names the first capability where the installed public reference map
/// disagrees with the decoded `language.reference` partition: a changed
/// program, a missing capability, or an extra one. This is the mutation
/// proof for post-compile map edits — `verify()` never trusts the map.
pub(super) fn first_installed_map_mismatch(
    installed: &BTreeMap<FeatureId, CompiledCell>,
    decoded: &BTreeMap<FeatureId, ReferenceEntry>,
) -> Option<FeatureId> {
    installed
        .iter()
        .find(|(feature, cell)| {
            decoded
                .get(feature)
                .map_or(true, |entry| &entry.cell != *cell)
        })
        .map(|(feature, _)| feature.clone())
        .or_else(|| {
            decoded
                .keys()
                .find(|feature| !installed.contains_key(feature))
                .cloned()
        })
}

pub(super) fn reference_entries_agree(left: &ReferenceEntry, right: &ReferenceEntry) -> bool {
    left.term == right.term && left.defaults == right.defaults && left.params == right.params && left.cell == right.cell
}

/// Names the first capability whose compiled entry the decoded page does
/// not reproduce byte-for-byte. `None` only when the tables agree.
pub(super) fn first_reference_mismatch(
    compiled: &BTreeMap<FeatureId, ReferenceEntry>,
    loaded: &BTreeMap<FeatureId, ReferenceEntry>,
) -> Option<FeatureId> {
    if compiled.len() == loaded.len()
        && compiled.iter().all(|(feature, entry)| {
            loaded
                .get(feature)
                .is_some_and(|other| reference_entries_agree(entry, other))
        })
    {
        return None;
    }
    compiled
        .iter()
        .find(|(feature, entry)| {
            !loaded
                .get(feature)
                .is_some_and(|other| reference_entries_agree(entry, other))
        })
        .map(|(feature, _)| feature.clone())
        .or_else(|| loaded.keys().next().cloned())
}


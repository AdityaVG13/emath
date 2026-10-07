//! Build-local record layouts: authored objects override the installed
//! Language Image for one emission, following the `ReferenceScope`
//! thread-local pattern (the image registry is global and shared;
//! authored records are per-build and must not leak into it).

use super::BTreeMap;

pub(super) type RecordFields = Vec<(String, String)>;

/// Integer state cannot depend on the width of its initializer.
/// Callable fields retain the same exact carrier in both directions.
pub(crate) fn stored_field_signature(signature: &str) -> String {
    fn retain(kind: super::ValueKind) -> super::ValueKind {
        use super::ValueKind;
        match kind {
            ValueKind::I64 => ValueKind::ExactInt,
            ValueKind::Vector(element) => ValueKind::Vector(Box::new(retain(*element))),
            ValueKind::Closure { params, result } => ValueKind::Closure {
                params: params.into_iter().map(retain).collect(),
                result: Box::new(retain(*result)),
            },
            other => other,
        }
    }
    super::kind_signature(&retain(super::ValueKind::from_signature(signature)))
        .unwrap_or_else(|| signature.to_string())
}

thread_local! {
    static AUTHORED_RECORDS: std::cell::RefCell<Vec<BTreeMap<String, RecordFields>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub(crate) struct AuthoredRecordScope;
impl AuthoredRecordScope {
    pub(crate) fn enter(records: BTreeMap<String, RecordFields>) -> Self {
        AUTHORED_RECORDS.with(|stack| stack.borrow_mut().push(records));
        Self
    }
}
impl Drop for AuthoredRecordScope {
    fn drop(&mut self) {
        AUTHORED_RECORDS.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// A generic authored object: its type parameters and field signatures
/// with each parameter spelled `Var<name>`.
#[derive(Clone)]
pub(crate) struct RecordTemplate {
    pub(crate) params: Vec<String>,
    pub(crate) fields: RecordFields,
}

thread_local! {
    static RECORD_TEMPLATES: std::cell::RefCell<BTreeMap<String, RecordTemplate>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
    /// Instantiations (`Pair<Int,Bool>`) whose layout was resolved
    /// during one crate emission: each needs its own struct.
    static INSTANTIATIONS: std::cell::RefCell<std::collections::BTreeSet<String>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// The generic objects of one crate emission. Leaving the scope drops
/// them and any instantiations not yet taken.
pub(crate) struct RecordTemplateScope(BTreeMap<String, RecordTemplate>);
impl RecordTemplateScope {
    pub(crate) fn enter(templates: BTreeMap<String, RecordTemplate>) -> Self {
        INSTANTIATIONS.with(|used| used.borrow_mut().clear());
        Self(RECORD_TEMPLATES.with(|table| table.replace(templates)))
    }
}
impl Drop for RecordTemplateScope {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.0);
        RECORD_TEMPLATES.with(|table| table.replace(previous));
        INSTANTIATIONS.with(|used| used.borrow_mut().clear());
    }
}

/// Every instantiation resolved since the last call, with its layout.
pub(crate) fn take_instantiations() -> Vec<(String, RecordFields)> {
    let names = INSTANTIATIONS.with(|used| std::mem::take(&mut *used.borrow_mut()));
    names
        .into_iter()
        .filter_map(|name| instantiated_layout(&name).map(|fields| (name, fields)))
        .collect()
}

/// Resolve every record a field signature names, so instantiations
/// nested in an instantiation's fields get their structs too.
pub(crate) fn touch_record_kinds(signature: &str) {
    fn walk(kind: &super::ValueKind) {
        use super::ValueKind;
        match kind {
            ValueKind::Record(name) => {
                let _ = record_layout(name);
            }
            ValueKind::Vector(inner) | ValueKind::Matrix(inner) => walk(inner),
            ValueKind::Result(ok, error) => {
                walk(ok);
                walk(error);
            }
            ValueKind::Closure { params, result } => {
                params.iter().for_each(walk);
                walk(result);
            }
            _ => {}
        }
    }
    walk(&super::ValueKind::from_signature(signature));
}

pub(crate) fn record_template(name: &str) -> Option<RecordTemplate> {
    RECORD_TEMPLATES.with(|table| table.borrow().get(name).cloned())
}

/// `Base<A,B>` with Base generic: the template's fields with each
/// parameter replaced by its argument. The argument count must match.
fn instantiated_layout(name: &str) -> Option<RecordFields> {
    let inner = name.strip_suffix('>')?;
    let (base, args) = inner.split_once('<')?;
    let template = record_template(base)?;
    let args = super::split_signature_parts(args);
    if args.len() != template.params.len() {
        return None;
    }
    let bound: BTreeMap<String, String> = template
        .params
        .iter()
        .cloned()
        .zip(args.iter().map(|arg| arg.trim().to_string()))
        .collect();
    Some(
        template
            .fields
            .iter()
            .map(|(field, signature)| {
                (
                    field.clone(),
                    stored_field_signature(&super::substitute_signature_vars(signature, &bound)),
                )
            })
            .collect(),
    )
}

/// Rust identifier suffix for a record name: an instantiation's
/// carrier punctuation folds to `_` (`Pair<Vector<Int>,Bool>` ->
/// `Pair_Vector_Int_Bool`).
pub(crate) fn record_ident(name: &str) -> String {
    if !name.contains('<') {
        return crate::rust_ir::ast::escape_ident(name);
    }
    let mut ident = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else if !ident.ends_with('_') {
            ident.push('_');
        }
    }
    ident.trim_end_matches('_').to_string()
}

/// The instantiation a literal of generic `base` builds, read off its
/// field carriers (`Pair: {left: 1, ..}` binds the parameter `left`
/// declares). None when `base` is not generic or a parameter stays
/// unbound.
pub(crate) fn infer_instantiation(base: &str, fields: &[(String, super::ValueKind)]) -> Option<String> {
    let template = record_template(base)?;
    let mut bound = BTreeMap::new();
    for (field, kind) in fields {
        if let Some((_, signature)) = template.fields.iter().find(|(name, _)| name == field) {
            super::bind_signature_vars(signature, kind, &mut bound);
        }
    }
    let args = template
        .params
        .iter()
        .map(|param| bound.get(param).cloned())
        .collect::<Option<Vec<_>>>()?;
    Some(format!("{base}<{}>", args.join(",")))
}

/// Field layouts (name → carrier signature) for a record name: the
/// innermost build-local authored table first, then a generic
/// instantiation, then the installed Language Image.
pub(crate) fn record_layout(name: &str) -> Option<RecordFields> {
    let authored = AUTHORED_RECORDS.with(|stack| {
        stack
            .borrow()
            .iter()
            .rev()
            .find_map(|table| table.get(name).cloned())
    });
    if authored.is_some() {
        return authored;
    }
    if let Some(fields) = instantiated_layout(name) {
        INSTANTIATIONS.with(|used| used.borrow_mut().insert(name.to_string()));
        return Some(fields);
    }
    emath_exec_ir::native_kernel::installed_record_layout(name).map(|layout| layout.fields)
}

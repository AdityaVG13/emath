//! Type and declaration encoding.

use super::*;

pub(super) fn encode_type_id(
    out: &mut Encoder,
    package: &SemanticPackage,
    id: TypeId,
) -> Result<(), MeaningError> {
    let ty = package
        .types
        .get(id.index())
        .ok_or(MeaningError::MissingType(id))?;
    encode_type(out, ty);
    Ok(())
}

pub(super) fn encode_type(out: &mut Encoder, ty: &TypeNode) {
    match ty {
        TypeNode::Bool => out.tag(0),
        TypeNode::Nat => out.tag(1),
        TypeNode::Int => out.tag(2),
        TypeNode::Rational => out.tag(3),
        TypeNode::Float64 => out.tag(4),
        TypeNode::Refinement { base, predicate } => {
            out.tag(5);
            encode_type(out, base);
            out.text(predicate);
        }
        TypeNode::Interval(inner) => {
            out.tag(6);
            encode_type(out, inner);
        }
        TypeNode::Complex(inner) => {
            out.tag(7);
            encode_type(out, inner);
        }
        TypeNode::Set(inner) => {
            out.tag(19);
            encode_type(out, inner);
        }
        TypeNode::Vector { element, extent } => {
            out.tag(8);
            encode_type(out, element);
            out.bool(extent.is_some());
            if let Some(extent) = extent {
                out.text(&extent.to_string());
            }
        }
        TypeNode::Matrix {
            element,
            rows,
            cols,
        } => {
            out.tag(9);
            encode_type(out, element);
            out.bool(rows.is_some());
            if let Some(rows) = rows {
                out.text(&rows.to_string());
            }
            out.bool(cols.is_some());
            if let Some(cols) = cols {
                out.text(&cols.to_string());
            }
        }
        TypeNode::Tensor { element, shape } => {
            out.tag(10);
            encode_type(out, element);
            out.usize(shape.len());
            for extent in shape {
                out.text(&extent.to_string());
            }
        }
        TypeNode::Record(name) => {
            out.tag(11);
            out.text(&name.0);
        }
        TypeNode::Variant(name) => {
            out.tag(12);
            out.text(&name.0);
        }
        TypeNode::Result { ok, error } => {
            out.tag(13);
            encode_type(out, ok);
            encode_type(out, error);
        }
        TypeNode::OptionType(inner) => {
            out.tag(14);
            encode_type(out, inner);
        }
        TypeNode::FieldPrime { modulus } => {
            out.tag(20);
            out.text(&modulus.to_string());
        }
        // Stage-2 (emath-t63iz): the exact big field-element node.
        TypeNode::BigInt => out.tag(21),
        TypeNode::Opaque {
            name,
            provider_contract,
        } => {
            out.tag(15);
            out.text(&name.0);
            out.bool(provider_contract.is_some());
            if let Some(contract) = provider_contract {
                out.text(&contract.0);
            }
        }
        TypeNode::UnitRef { dims, family, .. } => {
            out.tag(16);
            out.text(family.as_str());
            out.text(&dims.render());
        }
        TypeNode::Other(name) => {
            out.tag(17);
            out.text(&name.0);
        }
        TypeNode::Series { time, value } => {
            out.tag(18);
            encode_type(out, time);
            encode_type(out, value);
        }
    }
}

pub(super) fn alias_map(package: &SemanticPackage) -> BTreeMap<String, String> {
    let mut aliases = BTreeMap::new();
    for import in &package.imports {
        if let ImportSelection::Named(names) = &import.selection {
            for (name, alias) in names {
                let local = alias.as_ref().unwrap_or(name);
                let mut qualified = import.path.join("::");
                if !qualified.is_empty() {
                    qualified.push_str("::");
                }
                qualified.push_str(name);
                aliases.insert(local.clone(), qualified);
            }
        }
    }
    aliases
}

pub(super) fn add_fields(
    out: &mut Encoder,
    package: &SemanticPackage,
    locals: &mut BTreeMap<String, LocalSlot>,
    role: u8,
    fields: &[Field],
) -> Result<(), MeaningError> {
    out.tag(role);
    out.usize(fields.len());
    for (index, field) in fields.iter().enumerate() {
        encode_type_id(out, package, field.ty)?;
        out.tag(match field.visibility {
            Visibility::Public => 0,
            Visibility::Package => 1,
            Visibility::Private => 2,
        });
        locals.insert(field.name.clone(), LocalSlot { role, index });
    }
    Ok(())
}

pub(super) fn encode_exactness(out: &mut Encoder, policy: &ExactnessPolicy) {
    match policy {
        ExactnessPolicy::Exact => out.tag(0),
        ExactnessPolicy::Bounded { tolerance_literal } => {
            out.tag(1);
            out.text(tolerance_literal);
        }
        ExactnessPolicy::CheckedNumeric => out.tag(2),
        ExactnessPolicy::Estimate => out.tag(3),
        ExactnessPolicy::AnyExplicit => out.tag(4),
    }
}

pub(super) fn encode_declaration(
    package: &SemanticPackage,
    declaration: &Declaration,
    aliases: &BTreeMap<String, String>,
) -> Result<Vec<u8>, MeaningError> {
    let mut out = Encoder::default();
    out.text(
        aliases
            .get(&declaration.kind.0)
            .map_or(&declaration.kind.0, String::as_str),
    );
    let mut locals = BTreeMap::new();
    add_fields(&mut out, package, &mut locals, 0, &declaration.inputs)?;
    add_fields(&mut out, package, &mut locals, 1, &declaration.outputs)?;
    add_fields(&mut out, package, &mut locals, 2, &declaration.state)?;
    add_fields(&mut out, package, &mut locals, 3, &declaration.algebraic)?;

    let mut context = MeaningContext {
        package,
        locals,
        definitions: &declaration.definitions,
        aliases: aliases.clone(),
        bound: Vec::new(),
        active_exprs: BTreeSet::new(),
        active_definitions: BTreeSet::new(),
    };

    let mut definitions = Vec::new();
    for (name, expression) in &declaration.definitions {
        context.active_definitions.insert(name.clone());
        let mut encoded = Encoder::default();
        context.encode_expr(&mut encoded, *expression)?;
        context.active_definitions.remove(name);
        definitions.push(encoded.finish());
    }
    definitions.sort();
    out.tag(4);
    out.usize(definitions.len());
    for definition in definitions {
        out.blob(&definition);
    }

    out.tag(5);
    out.usize(declaration.invariants.len());
    for invariant in &declaration.invariants {
        context.encode_expr(&mut out, *invariant)?;
    }

    out.tag(6);
    out.usize(declaration.constructors.len());
    for constructor in &declaration.constructors {
        let declaration_locals = context.locals.clone();
        out.usize(constructor.parameters.len());
        for (index, parameter) in constructor.parameters.iter().enumerate() {
            encode_type_id(&mut out, package, parameter.ty)?;
            context
                .locals
                .insert(parameter.name.clone(), LocalSlot { role: 4, index });
        }
        out.usize(constructor.preconditions.len());
        for precondition in &constructor.preconditions {
            context.encode_expr(&mut out, *precondition)?;
        }
        out.usize(constructor.assignments.len());
        for (field, value) in &constructor.assignments {
            context.encode_name(&mut out, field)?;
            context.encode_expr(&mut out, *value)?;
        }
        out.usize(constructor.postconditions.len());
        for postcondition in &constructor.postconditions {
            context.encode_expr(&mut out, *postcondition)?;
        }
        out.usize(constructor.defaults.len());
        for (field, value) in &constructor.defaults {
            context.encode_name(&mut out, field)?;
            context.encode_expr(&mut out, *value)?;
        }
        out.bool(constructor.error_type.is_some());
        if let Some(error_type) = constructor.error_type {
            encode_type_id(&mut out, package, error_type)?;
        }
        out.bool(constructor.is_public);
        context.locals = declaration_locals;
    }

    out.tag(7);
    out.text(declaration.compile_spec.numeric.as_str());
    out.bool(declaration.compile_spec.unresolved.is_some());

    out.tag(8);
    out.usize(declaration.goals.len());
    for goal_id in &declaration.goals {
        let goal = package
            .goals
            .get(goal_id.index())
            .ok_or(MeaningError::MissingGoal(*goal_id))?;
        match &goal.kind {
            GoalKind::Custom(schema) => {
                out.tag(11);
                out.text(&schema.0);
            }
            kind => out.tag(match kind {
                GoalKind::Evaluate => 0,
                GoalKind::Differentiate => 1,
                GoalKind::Integrate => 2,
                GoalKind::Solve => 3,
                GoalKind::Optimize => 4,
                GoalKind::Simulate => 5,
                GoalKind::Search => 6,
                GoalKind::Prove => 7,
                GoalKind::Verify => 8,
                GoalKind::Compile => 9,
                GoalKind::Benchmark => 10,
                GoalKind::Transform => 11,
                GoalKind::Simplify => 12,
                GoalKind::Custom(_) => unreachable!(),
            }),
        }
        context.encode_name(&mut out, &goal.target)?;
        out.bool(goal.expression.is_some());
        if let Some(expression) = goal.expression {
            context.encode_expr(&mut out, expression)?;
        }
        encode_exactness(&mut out, &goal.requirements.exactness);
        out.tag(match goal.requirements.determinism {
            DeterminismPolicy::Required => 0,
            DeterminismPolicy::Preferred => 1,
            DeterminismPolicy::Unspecified => 2,
        });
        out.tag(match goal.requirements.fallback {
            FallbackPolicy::NativeOnly => 0,
            FallbackPolicy::Parametric => 1,
            FallbackPolicy::Continuation => 2,
            FallbackPolicy::Diagnostic => 3,
            FallbackPolicy::ExplicitLadder => 4,
        });
        out.text(&goal.requirements.target.family);
        out.bool(goal.requirements.target.triple.is_some());
        if let Some(triple) = &goal.requirements.target.triple {
            out.text(triple);
        }
        let mut features = goal.requirements.target.features.clone();
        features.sort();
        features.dedup();
        out.usize(features.len());
        for feature in features {
            out.text(&feature);
        }
        out.text(&goal.requirements.produce);
        out.usize(goal.payload.wrt.len());
        for name in &goal.payload.wrt {
            context.encode_name(&mut out, name)?;
        }
        out.bool(goal.payload.order.is_some());
        if let Some(order) = goal.payload.order {
            out.u64(u64::from(order));
        }
        out.bool(goal.payload.against.is_some());
        if let Some(against) = &goal.payload.against {
            out.text(against);
        }
        out.usize(goal.payload.measure.len());
        for name in &goal.payload.measure {
            context.encode_name(&mut out, name)?;
        }
    }

    out.tag(9);
    out.usize(declaration.exports.len());
    for export in &declaration.exports {
        out.text(&export.kind);
        context.encode_name(&mut out, &export.name)?;
        out.bool(export.is_public);
    }

    if let Some(residuals) = package.residuals.get(&declaration.id) {
        out.tag(10);
        out.usize(residuals.len());
        for residual in residuals {
            out.u64(u64::from(residual.components));
            context.encode_expr(&mut out, residual.expr)?;
            out.usize(residual.algebraic.len());
            for name in &residual.algebraic {
                context.encode_name(&mut out, name)?;
            }
            out.usize(residual.rates.len());
            for name in &residual.rates {
                context.encode_name(&mut out, name)?;
            }
        }
    } else {
        out.tag(10);
        out.usize(0);
    }
    Ok(out.finish())
}

/// Versioned, length-framed canonical bytes for admitted meaning.
pub fn canonical_meaning_bytes(
    package: &SemanticPackage,
    dependencies: &[MeaningId],
) -> Result<Vec<u8>, MeaningError> {
    // Package integrity, fail-closed: every interned capability
    // application must reference an interned cell.
    // Admission never produces a dangling reference, so a package that
    // carries one is malformed and cannot be assigned a MeaningID —
    // the orphan exprs are not walked by the declaration encoder, so
    // the validation is explicit here rather than emergent.
    for expr in &package.exprs {
        if let ExprNode::Apply { capability, .. } = expr {
            if package.capability(*capability).is_none() {
                return Err(MeaningError::MissingCapability(*capability));
            }
        }
    }
    let aliases = alias_map(package);
    let mut declarations = package
        .declarations
        .iter()
        .map(|declaration| encode_declaration(package, declaration, &aliases))
        .collect::<Result<Vec<_>, _>>()?;
    declarations.sort();

    let mut dependency_ids = dependencies
        .iter()
        .map(MeaningId::as_str)
        .collect::<Vec<_>>();
    dependency_ids.sort_unstable();
    dependency_ids.dedup();

    let mut out = Encoder::default();
    out.text(MEANING_CANONICAL_SCHEMA_V1);
    out.usize(declarations.len());
    for declaration in declarations {
        out.blob(&declaration);
    }
    out.usize(dependency_ids.len());
    for dependency in dependency_ids {
        out.text(dependency);
    }
    Ok(out.finish())
}

/// Cryptographic identity of canonical admitted meaning.
pub fn meaning_id(
    package: &SemanticPackage,
    dependencies: &[MeaningId],
) -> Result<MeaningId, MeaningError> {
    canonical_meaning_bytes(package, dependencies).map(|bytes| MeaningId::from_bytes(&bytes))
}

//! Type representation, refinement obligations and constrained inference.
//! Schemes cover primitives, generic records/variants, functions, opaque
//! capabilities; refinements carry explicit discharge status (static,
//! constructor, runtime-guard, certificate, external-assumption). Inference
//! keeps its own constraint store with a minimal unifier; diagnostics are
//! trace-bearing (`E-TYPE-3xx`).

use std::collections::BTreeMap;

use emath_core::QualifiedName;

/// A type variable in an inference context.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeVar(pub String);

/// A monomorphic-ish type expression with type variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeExpr {
    /// Free type variable.
    Var(TypeVar),
    /// Application of a named scheme (generics/records/variants).
    Con(QualifiedName, Vec<TypeExpr>),
    /// Function arrow.
    Arrow(Box<TypeExpr>, Box<TypeExpr>),
    /// Refined type: base plus a predicate with discharge status.
    Refined(Box<TypeExpr>, String, DischargeStatus),
}

/// How a refinement obligation is discharged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DischargeStatus {
    /// Verified statically by the compiler.
    Static,
    /// Enforced by a constructor invariant.
    Constructor,
    /// Emitted as a runtime guard.
    RuntimeGuard,
    /// Backed by a certificate.
    Certificate,
    /// Accepted as an external assumption.
    ExternalAssumption,
}

impl DischargeStatus {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Constructor => "constructor",
            Self::RuntimeGuard => "runtime-guard",
            Self::Certificate => "certificate",
            Self::ExternalAssumption => "external-assumption",
        }
    }

    /// Whether the obligation is a compile-time concern.
    #[must_use]
    pub const fn is_compile_time(self) -> bool {
        matches!(self, Self::Static | Self::Certificate)
    }
}

/// A field of a record or variant scheme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemeField {
    /// Field name.
    pub name: String,
    /// Field type expression.
    pub ty: TypeExpr,
}

/// Body of a type scheme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemeBody {
    /// Primitive type.
    Primitive,
    /// Generic record.
    Record(Vec<SchemeField>),
    /// Tagged variant.
    Variant(Vec<(String, Vec<SchemeField>)>),
    /// Function scheme.
    Function {
        /// Parameter types.
        parameters: Vec<TypeExpr>,
        /// Result type.
        result: Box<TypeExpr>,
    },
    /// Opaque capability with obligations.
    OpaqueCapability {
        /// Obligation names.
        obligations: Vec<String>,
    },
}

/// A named type scheme with generic parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeScheme {
    /// Scheme name.
    pub name: QualifiedName,
    /// Generic parameter names.
    pub generics: Vec<String>,
    /// Body.
    pub body: SchemeBody,
}

impl TypeScheme {
    /// Deterministic canonical rendering.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "scheme:{}<{}>:{body:?}",
            self.name.0,
            self.generics.join(","),
            body = self.body
        )
    }
}

/// Constraint store for type inference.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeConstraints {
    bindings: BTreeMap<TypeVar, TypeExpr>,
}

impl TypeConstraints {
    /// Binds a variable; conflicting bindings are typed errors.
    pub fn bind(&mut self, variable: TypeVar, ty: TypeExpr) -> Result<(), InferenceError> {
        if let Some(existing) = self.bindings.get(&variable) {
            if *existing != ty {
                return Err(InferenceError {
                    code: "E-TYPE-313",
                    message: format!(
                        "type variable `{}` bound to conflicting types {} and {}",
                        variable.0,
                        render(existing),
                        render(&ty)
                    ),
                });
            }
        } else {
            self.bindings.insert(variable, ty);
        }
        Ok(())
    }

    /// Lookup a binding.
    #[must_use]
    pub fn lookup(&self, variable: &TypeVar) -> Option<&TypeExpr> {
        self.bindings.get(variable)
    }

    /// Resolves a variable to its deepest binding.
    #[must_use]
    pub fn resolve(&self, expression: &TypeExpr) -> TypeExpr {
        let mut current = expression.clone();
        while let TypeExpr::Var(variable) = &current {
            match self.bindings.get(variable) {
                Some(next) => current = next.clone(),
                None => break,
            }
        }
        current
    }

    /// Number of live bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// Inference failure with a minimal conflict trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InferenceError {
    /// Stable code (`E-TYPE-312`/`E-TYPE-313`/`E-TYPE-314`).
    pub code: &'static str,
    /// Message (first conflict, deterministically rendered).
    pub message: String,
}

impl InferenceError {
    /// Full conflict trace (deterministic chain of unification attempts).
    #[must_use]
    pub fn trace(&self, context: &[&str]) -> String {
        format!("{} <- {}", self.message, context.join(" <- "))
    }
}

/// Unifies two type expressions, extending the store. Occurs-checked;
/// failure surfaces the first conflict in source order.
pub fn unify(
    store: &mut TypeConstraints,
    left: &TypeExpr,
    right: &TypeExpr,
) -> Result<(), InferenceError> {
    let left = store.resolve(left);
    let right = store.resolve(right);
    match (left, right) {
        (TypeExpr::Var(variable), other) | (other, TypeExpr::Var(variable)) => {
            if occurs(&variable, &other) {
                return Err(InferenceError {
                    code: "E-TYPE-314",
                    message: format!(
                        "occurs check: `{}` escapes into {}",
                        variable.0,
                        render(&other)
                    ),
                });
            }
            store.bind(variable, other)
        }
        (TypeExpr::Con(left_name, left_args), TypeExpr::Con(right_name, right_args)) => {
            if left_name != right_name || left_args.len() != right_args.len() {
                return Err(InferenceError {
                    code: "E-TYPE-312",
                    message: format!(
                        "cannot unify {} with {}",
                        render(&TypeExpr::Con(left_name, left_args.clone())),
                        render(&TypeExpr::Con(right_name, right_args.clone()))
                    ),
                });
            }
            for (l, r) in left_args.iter().zip(&right_args) {
                unify(store, l, r)?;
            }
            Ok(())
        }
        (TypeExpr::Arrow(l1, l2), TypeExpr::Arrow(r1, r2)) => {
            unify(store, &l1, &r1)?;
            unify(store, &l2, &r2)
        }
        (
            TypeExpr::Refined(base1, predicate, discharge1),
            TypeExpr::Refined(base2, predicate2, discharge2),
        ) => {
            if predicate != predicate2 {
                return Err(InferenceError {
                    code: "E-TYPE-312",
                    message: format!(
                        "refinement predicates `{predicate}` and `{predicate2}` differ"
                    ),
                });
            }
            // Discharge is part of the type's meaning: a statically
            // verified refinement must not unify with an
            // external-assumption one.
            if discharge1 != discharge2 {
                return Err(InferenceError {
                    code: "E-TYPE-312",
                    message: format!(
                        "refinement discharge `{}` and `{}` differ",
                        discharge1.name(),
                        discharge2.name()
                    ),
                });
            }
            unify(store, &base1, &base2)
        }
        (left, right) => Err(InferenceError {
            code: "E-TYPE-312",
            message: format!("cannot unify {} with {}", render(&left), render(&right)),
        }),
    }
}

fn occurs(variable: &TypeVar, ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Var(other) => variable == other,
        TypeExpr::Con(_, args) => args.iter().any(|arg| occurs(variable, arg)),
        TypeExpr::Arrow(left, right) => occurs(variable, left) || occurs(variable, right),
        TypeExpr::Refined(base, _, _) => occurs(variable, base),
    }
}

/// Deterministic rendering of a type expression.
#[must_use]
pub fn render(expression: &TypeExpr) -> String {
    match expression {
        TypeExpr::Var(variable) => variable.0.clone(),
        TypeExpr::Con(name, args) if args.is_empty() => name.0.clone(),
        TypeExpr::Con(name, args) => format!(
            "{}<{}>",
            name.0,
            args.iter().map(render).collect::<Vec<_>>().join(",")
        ),
        TypeExpr::Arrow(left, right) => format!("({} -> {})", render(left), render(right)),
        TypeExpr::Refined(base, predicate, discharge) => {
            format!("<{} {} {}>", predicate, render(base), discharge.name())
        }
    }
}

/// Canonical, injective rendering (used for type identity).
#[must_use]
pub fn canonical_of(expression: &TypeExpr) -> String {
    match expression {
        TypeExpr::Var(variable) => format!("var({})", variable.0),
        TypeExpr::Con(name, args) => format!(
            "con({},{})",
            name.0,
            args.iter().map(canonical_of).collect::<Vec<_>>().join(",")
        ),
        TypeExpr::Arrow(left, right) => {
            format!("arrow({},{})", canonical_of(left), canonical_of(right))
        }
        TypeExpr::Refined(base, predicate, discharge) => format!(
            "refined({},{},{})",
            canonical_of(base),
            predicate,
            discharge.name()
        ),
    }
}

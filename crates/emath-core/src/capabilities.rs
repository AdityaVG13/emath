//! Static compiler capability catalog.
//!
//! Admission, lowering, and backends consult this snapshot for what the
//! current Stage 1 engine claims. Schema version is bumped only when the
//! published shape of [`CompilerCapabilities`] changes.

/// Schema id for [`compiler_capabilities()`]. Bump on breaking catalog shape.
pub const COMPILER_CAPABILITIES_SCHEMA_V1: &str = "emath.compiler-capabilities.v1";

/// One admitted declaration-kind section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionDescriptor {
    pub name: &'static str,
    pub required: bool,
}

/// One numeric model the interpreter / rust-backend can evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumericModelDescriptor {
    pub name: &'static str,
    pub notes: &'static str,
}

/// One goal class the planner currently understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalDescriptor {
    pub name: &'static str,
    pub admitted: bool,
}

/// One world class name the kernel can name, even if no provider is wired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldClassDescriptor {
    pub name: &'static str,
    pub implemented: bool,
}

/// A named Stage 1+ feature that is not admitted yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeferredFeature {
    pub name: &'static str,
    pub reason: &'static str,
}

/// Snapshot of what this compiler build claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompilerCapabilities {
    pub schema: &'static str,
    pub kinds: &'static [&'static str],
    pub sections: &'static [SectionDescriptor],
    pub numeric_models: &'static [NumericModelDescriptor],
    pub goals: &'static [GoalDescriptor],
    pub worlds: &'static [WorldClassDescriptor],
    pub deferred: &'static [DeferredFeature],
}

const SECTIONS: &[SectionDescriptor] = &[
    SectionDescriptor {
        name: "inputs",
        required: false,
    },
    SectionDescriptor {
        name: "outputs",
        required: false,
    },
    SectionDescriptor {
        name: "state",
        required: false,
    },
    SectionDescriptor {
        name: "definitions",
        required: false,
    },
    SectionDescriptor {
        name: "equations",
        required: false,
    },
    SectionDescriptor {
        name: "constructors",
        required: false,
    },
    SectionDescriptor {
        name: "goals",
        required: false,
    },
];

const NUMERIC_MODELS: &[NumericModelDescriptor] = &[
    NumericModelDescriptor {
        name: "float64",
        notes: "default interpreter / rust-backend scalar",
    },
    NumericModelDescriptor {
        name: "vector-matrix",
        notes: "fixed-extent Vector[n] / Matrix[m, n] algebra",
    },
    NumericModelDescriptor {
        name: "ode-explicit",
        notes: "explicit der_<state> rates; Euler, RK4, and fixed-step RK45",
    },
    NumericModelDescriptor {
        name: "dae-implicit",
        notes: "algebraic: residual system Newton-solved per step (forward-difference Jacobian, Gaussian elimination; Euler + RK4 in interpreter and generated rust.library steps)",
    },
];

const GOALS: &[GoalDescriptor] = &[GoalDescriptor {
    name: "evaluate",
    admitted: true,
}];

const WORLDS: &[WorldClassDescriptor] = &[WorldClassDescriptor {
    name: "float64",
    implemented: true,
}];

const DEFERRED: &[DeferredFeature] = &[DeferredFeature {
    name: "autodiff",
    reason: "Track A3",
}];

/// Current compiler capability snapshot.
#[must_use]
pub const fn compiler_capabilities() -> CompilerCapabilities {
    CompilerCapabilities {
        schema: COMPILER_CAPABILITIES_SCHEMA_V1,
        kinds: &["function", "model"],
        sections: SECTIONS,
        numeric_models: NUMERIC_MODELS,
        goals: GOALS,
        worlds: WORLDS,
        deferred: DEFERRED,
    }
}

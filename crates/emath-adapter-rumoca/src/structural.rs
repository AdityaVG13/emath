//! Neutral structural model IR.
//!
//! Provider-free structural/equation IR for resolved and flattened dynamic
//! models: components, variables/parameters, units with dimensional
//! analysis, equations, derivatives, connections, initial conditions and
//! basic events. `canonical()` renders a deterministic form sealed by
//! FNV-1a64 content identity.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use emath_core::fnv1a64_bytes;
use emath_ir::TypeNode;

/// SI base dimension exponents (m, kg, s, A, K, mol, cd).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dimensions([i64; 7]);

impl Dimensions {
    /// Constructs from base dimension exponents.
    #[must_use]
    pub const fn base([m, kg, s, a, k, mol, cd]: [i64; 7]) -> Self {
        Self([m, kg, s, a, k, mol, cd])
    }

    /// Length (m).
    #[must_use]
    pub const fn meters() -> Self {
        Self([1, 0, 0, 0, 0, 0, 0])
    }

    /// Time (s).
    #[must_use]
    pub const fn seconds() -> Self {
        Self([0, 0, 1, 0, 0, 0, 0])
    }

    /// Velocity (m s^-1).
    #[must_use]
    pub const fn per_second() -> Self {
        Self([0, 0, -1, 0, 0, 0, 0])
    }

    /// Mass (kg).
    #[must_use]
    pub const fn kilograms() -> Self {
        Self([0, 1, 0, 0, 0, 0, 0])
    }

    /// Dimensionless.
    #[must_use]
    pub const fn dimensionless() -> Self {
        Self([0; 7])
    }

    /// Product of dimension exponents.
    #[must_use]
    pub const fn mul(self, other: Self) -> Self {
        let mut exponents = [0; 7];
        let mut index = 0;
        while index < 7 {
            exponents[index] = self.0[index] + other.0[index];
            index += 1;
        }
        Self(exponents)
    }

    /// Quotient of dimension exponents.
    #[must_use]
    pub const fn div(self, other: Self) -> Self {
        let mut exponents = [0; 7];
        let mut index = 0;
        while index < 7 {
            exponents[index] = self.0[index] - other.0[index];
            index += 1;
        }
        Self(exponents)
    }

    /// Reciprocal exponents.
    #[must_use]
    pub const fn inv(self) -> Self {
        let mut exponents = [0; 7];
        let mut index = 0;
        while index < 7 {
            exponents[index] = -self.0[index];
            index += 1;
        }
        Self(exponents)
    }

    /// Whether every exponent is zero.
    #[must_use]
    pub const fn is_dimensionless(self) -> bool {
        let mut index = 0;
        while index < 7 {
            if self.0[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }

    fn render(self) -> String {
        let mut parts = Vec::new();
        for (index, exponent) in self.0.iter().enumerate() {
            if *exponent != 0 {
                parts.push(format!("{}e{exponent}", SI_BASE_NAMES[index]));
            }
        }
        if parts.is_empty() {
            "1".to_string()
        } else {
            parts.join("*")
        }
    }
}

/// SI base unit names, one per dimension exponent.
pub const SI_BASE_NAMES: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

/// A unit with its dimension signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// Display name, e.g. "m/s".
    pub name: String,
    /// Dimension signature.
    pub dimensions: Dimensions,
}

impl Unit {
    /// Constructs a unit from name and dimensions.
    #[must_use]
    pub fn new(name: String, dimensions: Dimensions) -> Self {
        Self { name, dimensions }
    }

    /// The dimensionless unit.
    #[must_use]
    pub fn dimensionless() -> Self {
        Self {
            name: "1".to_string(),
            dimensions: Dimensions::dimensionless(),
        }
    }

    /// Meter unit.
    #[must_use]
    pub fn meters() -> Self {
        Self {
            name: "m".to_string(),
            dimensions: Dimensions::meters(),
        }
    }

    /// Second unit.
    #[must_use]
    pub fn seconds() -> Self {
        Self {
            name: "s".to_string(),
            dimensions: Dimensions::seconds(),
        }
    }

    /// Kilogram unit.
    #[must_use]
    pub fn kilograms() -> Self {
        Self {
            name: "kg".to_string(),
            dimensions: Dimensions::kilograms(),
        }
    }

    /// Renders `name [dimension signature]`.
    #[must_use]
    pub fn render(&self) -> String {
        format!("{}[{}]", self.name, self.dimensions.render())
    }
}

/// Variable role in a dynamic model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariableKind {
    /// Constant over the simulation horizon.
    Parameter,
    /// Time-varying state variable.
    State,
    /// Derived output.
    Output,
    /// Alias of another variable.
    Alias,
}

/// Component category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentKind {
    /// Composable component model.
    Model,
    /// Stackable component block.
    Block,
    /// Connection port type.
    Connector,
    /// Data record.
    Record,
}

/// A hierarchical component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    /// Stable local name.
    pub name: String,
    /// Category.
    pub kind: ComponentKind,
}

/// A model variable or parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableDecl {
    /// Stable local name.
    pub name: String,
    /// Role.
    pub kind: VariableKind,
    /// Unit.
    pub unit: Unit,
    /// Type.
    pub ty: TypeNode,
}

/// Differential-algebraic equation expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EqExpr {
    /// Plain variable reference.
    Var(String),
    /// Time derivative of a state.
    Der(String),
    /// Bit-exact literal (stored as bits for `Eq`).
    ConstF64(u64),
    /// Sum.
    Add(Box<EqExpr>, Box<EqExpr>),
    /// Difference.
    Sub(Box<EqExpr>, Box<EqExpr>),
    /// Product.
    Mul(Box<EqExpr>, Box<EqExpr>),
    /// Quotient.
    Div(Box<EqExpr>, Box<EqExpr>),
    /// Power.
    Pow(Box<EqExpr>, i32),
    /// Negation.
    Neg(Box<EqExpr>),
}

impl EqExpr {
    /// Literal from an f64 (bit-exact).
    #[must_use]
    pub fn constant(value: f64) -> Self {
        Self::ConstF64(value.to_bits())
    }

    /// Recovered f64 for literals.
    #[must_use]
    pub fn constant_value(&self) -> Option<f64> {
        match self {
            Self::ConstF64(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }

    /// All referenced variable and derivative names (deterministic order).
    #[must_use]
    pub fn identifiers(&self) -> Vec<String> {
        let mut found = BTreeSet::new();
        self.collect_identifiers(&mut found);
        found.into_iter().collect()
    }

    fn collect_identifiers(&self, found: &mut BTreeSet<String>) {
        match self {
            Self::Var(name) | Self::Der(name) => {
                found.insert(name.clone());
            }
            Self::ConstF64(_) => {}
            Self::Add(left, right)
            | Self::Sub(left, right)
            | Self::Mul(left, right)
            | Self::Div(left, right) => {
                left.collect_identifiers(found);
                right.collect_identifiers(found);
            }
            Self::Pow(base, _) | Self::Neg(base) => base.collect_identifiers(found),
        }
    }

    /// Dimensional analysis against a variable dimension environment.
    pub fn dimensions(
        &self,
        environment: &BTreeMap<String, Dimensions>,
    ) -> Result<Dimensions, UnitError> {
        match self {
            Self::Var(name) => environment
                .get(name)
                .copied()
                .ok_or_else(|| UnitError::unknown(name)),
            // A time derivative carries the state dimension divided by time.
            Self::Der(name) => match environment.get(name) {
                Some(dimensions) => Ok(dimensions.div(Dimensions::seconds())),
                None => Err(UnitError::unknown(name)),
            },
            Self::ConstF64(_) => Ok(Dimensions::dimensionless()),
            Self::Add(left, right) | Self::Sub(left, right) => {
                let left = left.dimensions(environment)?;
                let right = right.dimensions(environment)?;
                if left == right {
                    Ok(left)
                } else {
                    Err(UnitError::mismatch(&left.render(), &right.render()))
                }
            }
            Self::Mul(left, right) => Ok(left
                .dimensions(environment)?
                .mul(right.dimensions(environment)?)),
            Self::Div(left, right) => Ok(left
                .dimensions(environment)?
                .div(right.dimensions(environment)?)),
            Self::Pow(base, exponent) => {
                let base = base.dimensions(environment)?;
                Ok(Self::pow_dimensions(base, *exponent))
            }
            Self::Neg(inner) => inner.dimensions(environment),
        }
    }

    fn pow_dimensions(base: Dimensions, exponent: i32) -> Dimensions {
        let mut exponents = [0; 7];
        for (index, value) in exponents.iter_mut().enumerate() {
            *value = base.0[index] * i64::from(exponent);
        }
        Dimensions::base(exponents)
    }

    /// Deterministic structural rendering.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Var(name) => name.clone(),
            Self::Der(name) => format!("der({name})"),
            Self::ConstF64(bits) => {
                let value = f64::from_bits(*bits);
                format!("{value:e}")
            }
            Self::Add(left, right) => format!("({} + {})", left.canonical(), right.canonical()),
            Self::Sub(left, right) => format!("({} - {})", left.canonical(), right.canonical()),
            Self::Mul(left, right) => format!("({} * {})", left.canonical(), right.canonical()),
            Self::Div(left, right) => format!("({} / {})", left.canonical(), right.canonical()),
            Self::Pow(base, exponent) => format!("({} ^ {exponent})", base.canonical()),
            Self::Neg(inner) => format!("(-{})", inner.canonical()),
        }
    }
}

/// Dimensional analysis failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitError {
    /// Stable code (`E-UNIT-*`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl UnitError {
    fn unknown(name: &str) -> Self {
        Self {
            code: "E-UNIT-100",
            message: format!("unknown variable `{name}` in dimensional analysis"),
        }
    }

    fn mismatch(left: &str, right: &str) -> Self {
        Self {
            code: "E-UNIT-101",
            message: format!("dimension mismatch in sum: {left} vs {right}"),
        }
    }
}

/// A differential-algebraic equation with its component origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Equation {
    /// Left-hand side (usually `Var` or `Der`).
    pub lhs: EqExpr,
    /// Right-hand side.
    pub rhs: EqExpr,
    /// Origin component path for provenance.
    pub origin: String,
}

/// An initial value constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialCondition {
    /// Target state variable.
    pub target: String,
    /// Value expression.
    pub value: EqExpr,
}

/// A connection between two component ports (`component.port`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// Left port path.
    pub left: String,
    /// Right port path.
    pub right: String,
}

/// A basic (continuous-time) event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Stable event name.
    pub name: String,
    /// Dimensionless trigger condition.
    pub condition: EqExpr,
    /// Continuous-time event (Phase 1 subset: must be true).
    pub continuous: bool,
}

/// A structural model of a dynamic system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StructuralModel {
    /// Components.
    pub components: Vec<Component>,
    /// Variables and parameters.
    pub variables: Vec<VariableDecl>,
    /// Equations.
    pub equations: Vec<Equation>,
    /// Initial conditions.
    pub initial_conditions: Vec<InitialCondition>,
    /// Connections.
    pub connections: Vec<Connection>,
    /// Events.
    pub events: Vec<Event>,
}

/// Structural validation issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelIssue {
    /// Stable code.
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl StructuralModel {
    /// Validates structural invariants: unique names, derivative targets,
    /// connection endpoints/cardinality, initial-condition targets and
    /// per-equation dimensional consistency.
    #[must_use]
    pub fn validate(&self) -> Vec<ModelIssue> {
        let mut issues = Vec::new();
        let state_names: BTreeSet<String> = self
            .variables
            .iter()
            .filter(|variable| variable.kind == VariableKind::State)
            .map(|variable| variable.name.clone())
            .collect();
        let mut seen = BTreeSet::new();
        for variable in &self.variables {
            if !seen.insert(variable.name.clone()) {
                issues.push(ModelIssue {
                    code: "E-NAME-020",
                    message: format!("duplicate variable `{}`", variable.name),
                });
            }
        }
        let mut environment = BTreeMap::new();
        for variable in &self.variables {
            environment.insert(variable.name.clone(), variable.unit.dimensions);
        }
        let mut port_appearances: BTreeMap<String, usize> = BTreeMap::new();
        for connection in &self.connections {
            port_appearances
                .entry(connection.left.clone())
                .and_modify(|count| *count += 1)
                .or_insert(1);
            port_appearances
                .entry(connection.right.clone())
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
        for (port, count) in port_appearances {
            if count > 2 || !self.component_for_port(&port) {
                issues.push(ModelIssue {
                    code: "E-PROV-210",
                    message: format!(
                        "connection cardinality or unknown port `{port}` (appears {count}x)"
                    ),
                });
            }
        }
        for equation in &self.equations {
            self.check_equation(equation, &environment, &state_names, &mut issues);
        }
        for condition in &self.initial_conditions {
            if !state_names.contains(&condition.target) {
                issues.push(ModelIssue {
                    code: "E-TYPE-102",
                    message: format!(
                        "initial condition target `{}` is not a state variable",
                        condition.target
                    ),
                });
            }
        }
        for event in &self.events {
            match event.condition.dimensions(&environment) {
                Err(error) => {
                    // Keep the dimension-analysis refusal (`E-UNIT-100`/
                    // `E-UNIT-101`) instead of collapsing into `E-UNIT-103`.
                    issues.push(ModelIssue {
                        code: error.code,
                        message: format!("event `{}` condition: {}", event.name, error.message),
                    });
                }
                Ok(dimensions) if !dimensions.is_dimensionless() => {
                    issues.push(ModelIssue {
                        code: "E-UNIT-103",
                        message: format!("event `{}` condition is not dimensionless", event.name),
                    });
                }
                Ok(_) => {}
            }
        }
        issues
    }

    fn check_equation(
        &self,
        equation: &Equation,
        environment: &BTreeMap<String, Dimensions>,
        state_names: &BTreeSet<String>,
        issues: &mut Vec<ModelIssue>,
    ) {
        match &equation.lhs {
            EqExpr::Der(name) if !state_names.contains(name) => issues.push(ModelIssue {
                code: "E-TYPE-101",
                message: format!(
                    "derivative target `{name}` is not a state variable (equation {})",
                    self.equation_index(equation)
                ),
            }),
            EqExpr::Var(name) if state_names.contains(name) => issues.push(ModelIssue {
                code: "E-TYPE-103",
                message: format!(
                    "state `{name}` must not appear as a plain equation target (equation {})",
                    self.equation_index(equation)
                ),
            }),
            _ => {}
        }
        match (
            equation.lhs.dimensions(environment),
            equation.rhs.dimensions(environment),
        ) {
            (Ok(left), Ok(right)) if left == right => {}
            (Ok(left), Ok(right)) => issues.push(ModelIssue {
                code: "E-UNIT-101",
                message: format!(
                    "dimension mismatch in equation {}: {} vs {}",
                    self.equation_index(equation),
                    left.render(),
                    right.render()
                ),
            }),
            (Err(error), _) | (_, Err(error)) => issues.push(ModelIssue {
                code: error.code,
                message: error.message,
            }),
        }
    }

    fn equation_index(&self, equation: &Equation) -> usize {
        self.equations
            .iter()
            .position(|candidate| candidate == equation)
            .unwrap_or(usize::MAX)
    }

    fn component_for_port(&self, port: &str) -> bool {
        let Some((component, _)) = port.split_once('.') else {
            return false;
        };
        self.components.iter().any(|entry| entry.name == component)
    }

    /// Deterministic canonical rendering, byte-identical across runs.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        out.push_str("structural:{");
        out.push_str("components:[");
        for (index, component) in self.components.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{name:{},kind:{}}}",
                component.name,
                component_kind_name(component.kind)
            );
        }
        out.push_str("],variables:[");
        for (index, variable) in self.variables.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{name:{},kind:{},unit:{},ty:{}}}",
                variable.name,
                variable_kind_name(variable.kind),
                variable.unit.render(),
                variable.ty.display_name()
            );
        }
        out.push_str("],equations:[");
        for (index, equation) in self.equations.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{{}+{}+{}}}",
                equation.lhs.canonical(),
                equation.rhs.canonical(),
                equation.origin
            );
        }
        out.push_str("],initial:[");
        for (index, condition) in self.initial_conditions.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{{}+{}}}",
                condition.target,
                condition.value.canonical()
            );
        }
        out.push_str("],connections:[");
        for (index, connection) in self.connections.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{{}+{}}}", connection.left, connection.right);
        }
        out.push_str("],events:[");
        for (index, event) in self.events.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{name:{},condition:{},continuous:{}}}",
                event.name,
                event.condition.canonical(),
                event.continuous
            );
        }
        out.push_str("]}");
        out
    }

    /// FNV-1a64 content identity over the canonical rendering.
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64_bytes(self.canonical().as_bytes())
    }
}

fn component_kind_name(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Model => "model",
        ComponentKind::Block => "block",
        ComponentKind::Connector => "connector",
        ComponentKind::Record => "record",
    }
}

fn variable_kind_name(kind: VariableKind) -> &'static str {
    match kind {
        VariableKind::Parameter => "parameter",
        VariableKind::State => "state",
        VariableKind::Output => "output",
        VariableKind::Alias => "alias",
    }
}

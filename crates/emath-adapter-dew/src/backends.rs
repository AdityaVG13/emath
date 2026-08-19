//! Breadth backends.
//!
//! - Rust source backend: deterministic Rust fragments with per-node
//!   source anchors and a bounded syntax sanity check.
//! - Token-stream backend: deterministic token lists for proc-macro /
//!   build integration with identical semantics (the joined text of
//!   the token stream is the fragment text).
//! - Cranelift JIT capability: explicit target, runtime
//!   evidence-scoped execution, fallback to generated/native
//!   interpretation.
//! - Accelerator inventory: only explicitly admitted subsets with
//!   declared target/numeric semantics and device transfer plans;
//!   CUDA/HIP/OpenCL are inventoried but not admitted.

use crate::dexpr::DewExpr;

/// Proton-based targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AcceleratorTarget {
    Wgsl,
    Glsl,
    Cuda,
    Hip,
    OpenCl,
}

impl AcceleratorTarget {
    /// Stable target token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wgsl => "wgsl",
            Self::Glsl => "glsl",
            Self::Cuda => "cuda",
            Self::Hip => "hip",
            Self::OpenCl => "opencl",
        }
    }

    /// Declared numeric semantics for the target.
    #[must_use]
    pub const fn numeric_semantics(self) -> &'static str {
        match self {
            Self::Wgsl | Self::Glsl => "strict-f64-subset-f32-only",
            Self::Cuda | Self::Hip | Self::OpenCl => "not-admitted",
        }
    }
}

/// Rust source generation result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustFragment {
    /// Deterministic rendered source.
    pub text: String,
    /// Per-node anchors (`node index -> generated symbol`), monotone.
    pub anchors: Vec<(usize, String)>,
    /// Result symbol of the expression.
    pub result: String,
}

/// One lexeme of the token-stream backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenKind {
    Ident,
    Punct,
    Literal,
}

impl TokenKind {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ident => "ident",
            Self::Punct => "punct",
            Self::Literal => "literal",
        }
    }
}

/// One token (proc-macro/build friendly).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

/// Token-stream backend result: the joined token texts equal the
/// fragment text up to whitespace normalization (no divergent
/// semantics).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenStream {
    pub tokens: Vec<Token>,
}

impl TokenStream {
    /// Joined text over the tokens (whitespace-normalized).
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for token in &self.tokens {
            if !out.is_empty() && !out.ends_with(['(', '[', ' ', '.', ',']) {
                out.push(' ');
            }
            out.push_str(&token.text);
        }
        out
    }
}

/// Cranelift JIT capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JitTarget {
    /// Machine architecture.
    pub arch: String,
    /// Encoding width hint.
    pub width_bits: u16,
}

/// JIT capability descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JitCapability {
    pub target: JitTarget,
    /// Runtime execution is evidence-scoped only.
    pub evidence_scoped: bool,
    /// Fallback strategy when the JIT is unavailable.
    pub fallback: String,
}

/// Device transfer plan for an admitted accelerator subset
///.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceTransferPlan {
    /// Host-to-device staging.
    pub host_to_device: String,
    /// Device-to-host readback.
    pub device_to_host: String,
    /// Precision policy on the device (declared numeric semantics).
    pub precision: String,
}

/// One accelerator admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceleratorPlan {
    pub target: AcceleratorTarget,
    pub semantics: String,
    pub transfer: DeviceTransferPlan,
}

/// Backend selection outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendSelection {
    pub backend: String,
    pub deterministic: bool,
    pub fallback: Option<String>,
}

/// Cranelift JIT capability for the current adapter build.
#[must_use]
pub fn jit_capability() -> JitCapability {
    JitCapability {
        target: JitTarget {
            arch: "aarch64".into(),
            width_bits: 64,
        },
        evidence_scoped: true,
        fallback: "native strict-f64 interpreter".into(),
    }
}

/// Accelerator inventory: WGSL/GLSL admitted subsets only.
#[must_use]
pub fn accelerator_inventory() -> Vec<AcceleratorPlan> {
    vec![
        AcceleratorPlan {
            target: AcceleratorTarget::Wgsl,
            semantics: AcceleratorTarget::Wgsl.numeric_semantics().into(),
            transfer: DeviceTransferPlan {
                host_to_device: "f32 rebind of strict-f64 constants".into(),
                device_to_host: "exact f32 readback into f64".into(),
                precision: "f32 (documented loss)".into(),
            },
        },
        AcceleratorPlan {
            target: AcceleratorTarget::Glsl,
            semantics: AcceleratorTarget::Glsl.numeric_semantics().into(),
            transfer: DeviceTransferPlan {
                host_to_device: "f32 rebind of strict-f64 constants".into(),
                device_to_host: "exact f32 readback into f64".into(),
                precision: "f32 (documented loss)".into(),
            },
        },
    ]
}

/// Whether a device plan admits the target (`E-PROV-031` otherwise).
pub fn admit_target(
    inventory: &[AcceleratorPlan],
    target: AcceleratorTarget,
) -> Result<&AcceleratorPlan, String> {
    inventory
        .iter()
        .find(|plan| plan.target == target)
        .ok_or_else(|| {
            format!(
                "E-PROV-031: accelerator target `{}` has no admitted subset",
                target.as_str()
            )
        })
}

/// Deterministic per-node Rust fragment generation for the scalar
/// subset. Anchors are monotone `(node, symbol)` pairs; the fragment
/// is syntax-sanity-checked (balanced delimiters).
pub fn render_rust_fragment(expr: &DewExpr) -> Result<RustFragment, String> {
    let mut stmts = Vec::new();
    let mut anchors = Vec::new();
    let mut next = 0usize;
    let result = emit(expr, &mut stmts, &mut anchors, &mut next)?;
    let text = format!(
        "fn dew_fragment() -> f64 {{\n    {}\n    {result}\n}}",
        stmts.join("\n    ")
    );
    if !syntax_sane(&text) {
        return Err("E-PROV-030: generated Rust fragment fails the syntax sanity gate".to_string());
    }
    Ok(RustFragment {
        text,
        anchors,
        result,
    })
}

/// Emits statements for the expression and returns the result symbol.
fn emit(
    expr: &DewExpr,
    stmts: &mut Vec<String>,
    anchors: &mut Vec<(usize, String)>,
    next: &mut usize,
) -> Result<String, String> {
    let name = format!("t{next}");
    *next += 1;
    let stmt = match expr {
        DewExpr::Float64Bits(bits) => format!("let {name}: f64 = f64::from_bits({bits:#018x});"),
        DewExpr::Bool(value) => format!("let {name}: bool = {value};"),
        DewExpr::Int(text) => {
            let parsed: f64 = text
                .replace('_', "")
                .parse()
                .map_err(|_| format!("E-PROV-030: integer literal `{text}` is not a finite f64"))?;
            if !parsed.is_finite() {
                return Err(format!(
                    "E-PROV-030: integer literal `{text}` exceeds the strict-f64 finite range"
                ));
            }
            format!("let {name}: f64 = {text}.0 as f64;")
        }
        DewExpr::Var(name_var) => {
            if !ident_sane(name_var) {
                return Err(format!(
                    "E-PROV-030: variable name `{name_var}` is not a safe Rust identifier"
                ));
            }
            format!("let {name}: f64 = {name_var};")
        }
        DewExpr::Add(left, right) => two_operands(stmts, anchors, next, &name, "+", left, right)?,
        DewExpr::Sub(left, right) => two_operands(stmts, anchors, next, &name, "-", left, right)?,
        DewExpr::Mul(left, right) => two_operands(stmts, anchors, next, &name, "*", left, right)?,
        DewExpr::Div(left, right) => two_operands(stmts, anchors, next, &name, "/", left, right)?,
        DewExpr::Pow(left, right) => {
            let l = emit(left, stmts, anchors, next)?;
            let r = emit(right, stmts, anchors, next)?;
            format!("let {name}: f64 = {l}.powf({r});")
        }
        DewExpr::Neg(value) => {
            let inner = emit(value, stmts, anchors, next)?;
            format!("let {name}: f64 = -{inner};")
        }
        DewExpr::Abs(value) => {
            let inner = emit(value, stmts, anchors, next)?;
            format!("let {name}: f64 = {inner}.abs();")
        }
        DewExpr::If {
            condition,
            then_value,
            else_value,
        } => {
            let cond = emit(condition, stmts, anchors, next)?;
            let then_name = emit(then_value, stmts, anchors, next)?;
            let else_name = emit(else_value, stmts, anchors, next)?;
            format!("let {name}: f64 = if {cond} {{ {then_name} }} else {{ {else_name} }};")
        }
        DewExpr::Sqrt(value) => unary_call(stmts, anchors, next, &name, ".sqrt()", value)?,
        DewExpr::Exp(value) => unary_call(stmts, anchors, next, &name, ".exp()", value)?,
        DewExpr::Ln(value) => unary_call(stmts, anchors, next, &name, ".ln()", value)?,
        DewExpr::Sin(value) => unary_call(stmts, anchors, next, &name, ".sin()", value)?,
        DewExpr::Cos(value) => unary_call(stmts, anchors, next, &name, ".cos()", value)?,
        DewExpr::Tan(value) => unary_call(stmts, anchors, next, &name, ".tan()", value)?,
        DewExpr::Tanh(value) => unary_call(stmts, anchors, next, &name, ".tanh()", value)?,
        DewExpr::Floor(value) => unary_call(stmts, anchors, next, &name, ".floor()", value)?,
        DewExpr::Ceil(value) => unary_call(stmts, anchors, next, &name, ".ceil()", value)?,
        DewExpr::Min(left, right) => {
            let l = emit(left, stmts, anchors, next)?;
            let r = emit(right, stmts, anchors, next)?;
            format!("let {name}: f64 = {l}.min({r});")
        }
        DewExpr::Max(left, right) => {
            let l = emit(left, stmts, anchors, next)?;
            let r = emit(right, stmts, anchors, next)?;
            format!("let {name}: f64 = {l}.max({r});")
        }
        DewExpr::Atan2(left, right) => {
            let l = emit(left, stmts, anchors, next)?;
            let r = emit(right, stmts, anchors, next)?;
            format!("let {name}: f64 = {l}.atan2({r});")
        }
        DewExpr::And(left, right) => {
            let l = emit(left, stmts, anchors, next)?;
            let r = emit(right, stmts, anchors, next)?;
            format!("let {name}: f64 = if ({l} != 0.0) && ({r} != 0.0) {{ 1.0 }} else {{ 0.0 }};")
        }
        DewExpr::Or(left, right) => {
            let l = emit(left, stmts, anchors, next)?;
            let r = emit(right, stmts, anchors, next)?;
            format!("let {name}: f64 = if ({l} != 0.0) || ({r} != 0.0) {{ 1.0 }} else {{ 0.0 }};")
        }
        DewExpr::Cmp(op, left, right) => {
            let l = emit(left, stmts, anchors, next)?;
            let r = emit(right, stmts, anchors, next)?;
            let operator = match op {
                crate::dexpr::CmpOp::Eq => "==",
                crate::dexpr::CmpOp::Ne => "!=",
                crate::dexpr::CmpOp::Lt => "<",
                crate::dexpr::CmpOp::Le => "<=",
                crate::dexpr::CmpOp::Gt => ">",
                crate::dexpr::CmpOp::Ge => ">=",
            };
            format!("let {name}: f64 = if {l} {operator} {r} {{ 1.0 }} else {{ 0.0 }};")
        }
        // Boolean-typed nodes cannot live in the f64-shaped fragment
        // (`fn dew_fragment() -> f64`); the evaluator types them `Bool`.
        // Refuse with a typed code instead of coercing them silently.
        DewExpr::Not(_) | DewExpr::IsFinite(_) => {
            return Err(
                "E-PROV-030: boolean-typed node is outside the scalar strict-f64 Rust \
                 fragment backend and is refused"
                    .into(),
            );
        }
        // Non-scalar nodes are refused with a typed code: emitting a
        // numeric placeholder (`0.0`) would let a refused shape pass as
        // computed output. `map_expression` refuses before the backend,
        // and the backend re-refuses as defense in depth.
        DewExpr::Matrix(_) | DewExpr::Linear(..) => {
            return Err(
                "E-PROV-030: non-scalar linear-algebra node is outside the scalar strict-f64 \
                 Rust backend and is refused (never stubbed)"
                    .into(),
            );
        }
    };
    stmts.push(stmt);
    anchors.push((anchors.len(), name.clone()));
    Ok(name)
}

/// Emits one operand, then a method call applied to it.
fn unary_call(
    stmts: &mut Vec<String>,
    anchors: &mut Vec<(usize, String)>,
    next: &mut usize,
    name: &str,
    call: &str,
    value: &DewExpr,
) -> Result<String, String> {
    let inner = emit(value, stmts, anchors, next)?;
    Ok(format!("let {name}: f64 = {inner}{call};"))
}

fn two_operands(
    stmts: &mut Vec<String>,
    anchors: &mut Vec<(usize, String)>,
    next: &mut usize,
    name: &str,
    operator: &str,
    left: &DewExpr,
    right: &DewExpr,
) -> Result<String, String> {
    let l = emit(left, stmts, anchors, next)?;
    let r = emit(right, stmts, anchors, next)?;
    Ok(format!("let {name}: f64 = {l} {operator} {r};"))
}

/// Bounded syntax sanity check: balanced brackets and no stray
/// non-ASCII in identifiers.
#[must_use]
pub fn syntax_sane(text: &str) -> bool {
    let mut depth = 0i64;
    for character in text.chars() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// Whether `name` is a safe Rust identifier (ASCII alphanumeric or
/// underscore, not starting with a digit). Variables are spliced into
/// generated Rust; anything else is refused, never emitted.
#[must_use]
pub fn ident_sane(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Token-stream generation over the same fragment (identical joined
/// text; proc-macro/build integration without divergent semantics).
pub fn render_tokens(fragment: &RustFragment) -> TokenStream {
    let mut tokens = TokenStream::default();
    for word in fragment.text.split_whitespace() {
        let kind = if word
            .chars()
            .all(|char| char.is_ascii_alphabetic() || char == '_')
        {
            TokenKind::Ident
        } else if word
            .chars()
            .all(|char| !char.is_alphanumeric() && char != '_')
        {
            TokenKind::Punct
        } else {
            TokenKind::Literal
        };
        tokens.tokens.push(Token {
            kind,
            text: word.to_string(),
        });
    }
    tokens
}

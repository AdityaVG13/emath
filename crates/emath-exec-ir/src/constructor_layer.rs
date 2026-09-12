//! Constructor-layer evaluation: object, function, recur, quote, query.
//!
//! Reuses the shared syntax tree and scalar carrier arithmetic. Mathematical
//! recipes are ordinary module functions, not image FeatureIDs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use emath_core::tree::{
    BinaryOp, Declaration, Expr, ExprKind, Item, StmtKind, SyntaxTree, TypeExpr, TypeKind, UnaryOp,
    UseTree,
};

const DEFAULT_WORK: u64 = 1_000_000;

#[derive(Clone, Debug, PartialEq)]
pub enum CValue {
    Bool(bool),
    Int(i128),
    Rat { num: i128, den: i128 },
    Float64(f64),
    Sequence(Vec<CValue>),
    Tuple(Vec<CValue>),
    Record {
        type_name: String,
        fields: BTreeMap<String, CValue>,
    },
    Variant {
        type_name: String,
        tag: String,
        fields: Vec<CValue>,
    },
    Closure(Box<Closure>),
    Code(Box<Code>),
    Receipt(Box<Receipt>),
    Unit,
    Absent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    pub param: String,
    pub body: Expr,
    pub env: BTreeMap<String, CValue>,
    pub recursive: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Code {
    pub expr: Expr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub execution: String,
    pub fulfillment: String,
    pub representation: String,
    pub payload: Option<String>,
    pub evidence: Vec<String>,
    pub remaining: Vec<String>,
    pub diagnostic_code: Option<String>,
}

impl fmt::Display for CValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(v) => write!(f, "{v}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Rat { num, den } => write!(f, "{num}/{den}"),
            Self::Float64(v) => write!(f, "{v}"),
            Self::Sequence(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            Self::Tuple(xs) => {
                write!(f, "(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, ")")
            }
            Self::Record { type_name, fields } => {
                write!(f, "{type_name}(")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}={v}")?;
                }
                write!(f, ")")
            }
            Self::Variant {
                type_name,
                tag,
                fields,
            } => {
                write!(f, "{type_name}.{tag}(")?;
                for (i, v) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ")")
            }
            Self::Closure(_) => write!(f, "<closure>"),
            Self::Code(_) => write!(f, "<code>"),
            Self::Receipt(r) => write!(
                f,
                "receipt(execution={}, fulfillment={}, representation={})",
                r.execution, r.fulfillment, r.representation
            ),
            Self::Unit => write!(f, "()"),
            Self::Absent => write!(f, "Absent"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorError {
    pub code: String,
    pub message: String,
}

impl fmt::Display for ConstructorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestObservation {
    pub label: String,
    pub passed: bool,
    pub detail: String,
    pub receipt: Option<Receipt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleReport {
    pub bindings: BTreeMap<String, CValue>,
    pub tests: Vec<TestObservation>,
    pub uses: Vec<String>,
}

#[derive(Clone)]
struct FnDecl {
    inputs: Vec<String>,
    input_types: Vec<TypeExpr>,
    outputs: Vec<String>,
    output_types: Vec<TypeExpr>,
    output: Option<String>,
    defs: Vec<(String, Expr)>,
    opaque: bool,
}

#[derive(Clone)]
struct QueryDecl {
    inputs: Vec<String>,
    defs: Vec<(String, Expr)>,
    form: String,
    method: Option<String>,
    accept: Option<String>,
}

/// Remaining-work continuation slot. A budget stop keeps these with the
/// frame; resume applies them instead of replaying completed reductions.
#[derive(Clone, Debug, PartialEq)]
pub enum Kont {
    BinLeft {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    BinRight {
        op: BinaryOp,
        left: CValue,
        right: Box<Expr>,
    },
    IfAfterCond {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    IfThen {
        then_value: Box<Expr>,
    },
    IfElse {
        else_value: Box<Expr>,
    },
    CallArgs {
        callee: CValue,
        done: Vec<CValue>,
        rest: Vec<Expr>,
    },
    FnCall {
        name: String,
        done: Vec<CValue>,
        rest: Vec<Expr>,
    },
    SeqItems {
        as_tuple: bool,
        done: Vec<CValue>,
        rest: Vec<Expr>,
    },
    RecordFields {
        type_path: Vec<String>,
        done: BTreeMap<String, CValue>,
        current: String,
        current_expr: Box<Expr>,
        rest: Vec<(String, Expr)>,
    },
    IndexAfterSeq {
        seq: CValue,
        index: Box<Expr>,
    },
    UnaryAfter {
        op: UnaryOp,
        value: Box<Expr>,
    },
    ConsLeft {
        head: Box<Expr>,
        tail: Box<Expr>,
    },
    ConsAfterHead {
        head: CValue,
        tail: Box<Expr>,
    },
    MatchWaiting {
        subject: Box<Expr>,
        arms: Vec<(Expr, Expr)>,
        else_arm: Box<Expr>,
    },
    CasesArm {
        cond: Box<Expr>,
        value: Box<Expr>,
        rest: Vec<(Expr, Expr)>,
        else_arm: Box<Expr>,
    },
    OpenAfterPacked {
        type_name: String,
        param: String,
        packed: Box<Expr>,
        body: Box<Expr>,
    },
    EvalExpr {
        expr: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContinuationFrame {
    pub function: String,
    pub pc: u64,
    /// Next unfinished definition or instruction label in this frame.
    pub next: String,
    pub env: BTreeMap<String, CValue>,
    pub kont: Vec<Box<Kont>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    pub schema: String,
    pub source_id: String,
    pub image: String,
    pub abi: String,
    pub work: u64,
    pub remaining: u64,
    pub accounting: String,
    pub memo: BTreeMap<String, CValue>,
    pub frames: Vec<ContinuationFrame>,
    pub scopes: BTreeSet<u64>,
    pub function: String,
    pub source: String,
    pub inputs: BTreeMap<String, CValue>,
    pub next_ref: u64,
}

pub const CHECKPOINT_SCHEMA: &str = "emath.constructor-checkpoint.v1";
pub const CHECKPOINT_ABI: &str = "constructor-layer/continuation-abi";
pub const ACCOUNTING_VERSION: &str = "constructor-layer/accounting.v1";
pub const IMAGE_IDENTITY: &str = "constructor-layer";

impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            schema: CHECKPOINT_SCHEMA.into(),
            source_id: String::new(),
            image: IMAGE_IDENTITY.into(),
            abi: CHECKPOINT_ABI.into(),
            work: 0,
            remaining: 0,
            accounting: ACCOUNTING_VERSION.into(),
            memo: BTreeMap::new(),
            frames: Vec::new(),
            scopes: BTreeSet::new(),
            function: String::new(),
            source: String::new(),
            inputs: BTreeMap::new(),
            next_ref: 0,
        }
    }
}

struct Engine {
    env: BTreeMap<String, CValue>,
    functions: BTreeMap<String, FnDecl>,
    queries: BTreeMap<String, QueryDecl>,
    objects: BTreeMap<String, ObjectSchema>,
    work: u64,
    work_limit: u64,
    memo: BTreeMap<String, CValue>,
    visit: usize,
    source_id: String,
    entry: String,
    source_text: String,
    inputs: BTreeMap<String, CValue>,
    call_depth: u32,
    next_ref: u64,
    frames: Vec<ContinuationFrame>,
    scopes: BTreeSet<u64>,
    resume_frames: Vec<ContinuationFrame>,
}

#[derive(Clone)]
struct ObjectSchema {
    kind: String,
    invariants: Vec<(String, Expr)>,
}

impl Engine {
    fn charge(&mut self) -> Result<(), ConstructorError> {
        self.work += 1;
        if self.work > self.work_limit {
            return Err(ConstructorError {
                code: "budget_exhausted".into(),
                message: "work budget exhausted".into(),
            });
        }
        Ok(())
    }

    fn push_kont(&mut self, kont: Kont) {
        if let Some(frame) = self.frames.last_mut() {
            frame.kont.push(Box::new(kont));
        }
    }

    fn pop_kont(&mut self) -> Option<Box<Kont>> {
        self.frames.last_mut()?.kont.pop()
    }

    fn eval(&mut self, expr: &Expr) -> Result<CValue, ConstructorError> {
        self.visit += 1;
        self.refresh_frame();
        self.charge()?;
        self.eval_fresh(expr)
    }

    fn eval_fresh(&mut self, expr: &Expr) -> Result<CValue, ConstructorError> {
        match &expr.kind {
            ExprKind::Int(text) => parse_int(text),
            ExprKind::Rational { numer, denom } => {
                let num = parse_i128(numer)?;
                let den = parse_i128(denom)?;
                if den == 0 {
                    return Err(fault("division_by_zero", "rational denominator is zero"));
                }
                Ok(CValue::Rat { num, den: den.abs() }.canon())
            }
            ExprKind::Float(text) => {
                let cleaned = text.trim_end_matches("f64").trim_end_matches("Float64");
                cleaned
                    .parse::<f64>()
                    .map(CValue::Float64)
                    .map_err(|_| fault("invalid_literal", format!("not Float64: {text}")))
            }
            ExprKind::Bool(v) => Ok(CValue::Bool(*v)),
            ExprKind::Path { segments, .. } => {
                let name = segments.join(".");
                if let Some(value) = self.env.get(&name).cloned() {
                    return Ok(value);
                }
                if segments.len() == 2 {
                    if let Some(CValue::Sequence(items)) = self.env.get(&segments[0]) {
                        if segments[1] == "length" {
                            return Ok(CValue::Int(items.len() as i128));
                        }
                    }
                    if let Some(CValue::Record { fields, .. }) = self.env.get(&segments[0]) {
                        if let Some(field) = fields.get(&segments[1]) {
                            return Ok(field.clone());
                        }
                    }
                    if let Some(CValue::Tuple(items)) = self.env.get(&segments[0]) {
                        if let Some(index) = tuple_index(&segments[1]) {
                            if let Some(item) = items.get(index) {
                                return Ok(item.clone());
                            }
                        }
                    }
                    if let Some(CValue::Receipt(receipt)) = self.env.get(&segments[0]) {
                        return match segments[1].as_str() {
                            "execution" => Ok(CValue::Record {
                                type_name: receipt.execution.clone(),
                                fields: BTreeMap::new(),
                            }),
                            "fulfillment" => Ok(CValue::Record {
                                type_name: receipt.fulfillment.clone(),
                                fields: BTreeMap::new(),
                            }),
                            "representation" => Ok(CValue::Record {
                                type_name: receipt.representation.clone(),
                                fields: BTreeMap::new(),
                            }),
                            "payload" => Ok(CValue::Record {
                                type_name: receipt.payload.clone().unwrap_or_default(),
                                fields: BTreeMap::new(),
                            }),
                            "remaining" => Ok(CValue::Sequence(
                                receipt
                                    .remaining
                                    .iter()
                                    .map(|item| CValue::Record {
                                        type_name: item.clone(),
                                        fields: BTreeMap::new(),
                                    })
                                    .collect(),
                            )),
                            "evidence" => Ok(CValue::Sequence(
                                receipt
                                    .evidence
                                    .iter()
                                    .map(|item| CValue::Record {
                                        type_name: item.clone(),
                                        fields: BTreeMap::new(),
                                    })
                                    .collect(),
                            )),
                            _ => Err(fault(
                                "unbound",
                                format!("receipt has no field `{}`", segments[1]),
                            )),
                        };
                    }
                }
                if name == "_unmatched" {
                    return Err(fault("nonexhaustive_match", "match did not cover the scrutinee"));
                }
                if name == "true" {
                    return Ok(CValue::Bool(true));
                }
                if name == "false" {
                    return Ok(CValue::Bool(false));
                }
                if is_schema_tag(&name) {
                    return Ok(schema_tag(&name));
                }
                if self.functions.contains_key(&name) {
                    return Ok(CValue::Closure(Box::new(Closure {
                        param: String::new(),
                        body: expr.clone(),
                        env: self.env.clone(),
                        recursive: Some(name),
                    })));
                }
                Err(fault("unbound", format!("unbound `{name}`")))
            }
            ExprKind::Unary { op, value } => {
                self.push_kont(Kont::UnaryAfter {
                    op: *op,
                    value: value.clone(),
                });
                let v = self.eval(value)?;
                self.pop_kont();
                apply_unary(*op, v)
            }
            ExprKind::Binary { op, left, right } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    self.push_kont(Kont::BinLeft {
                        op: *op,
                        left: left.clone(),
                        right: right.clone(),
                    });
                    let l = self.eval(left)?;
                    self.pop_kont();
                    return match (op, l) {
                        (BinaryOp::And, CValue::Bool(false)) => Ok(CValue::Bool(false)),
                        (BinaryOp::Or, CValue::Bool(true)) => Ok(CValue::Bool(true)),
                        (BinaryOp::And, CValue::Bool(true)) => self.eval(right),
                        (BinaryOp::Or, CValue::Bool(false)) => self.eval(right),
                        _ => Err(fault("type", "boolean combinator expects Bool")),
                    };
                }
                self.push_kont(Kont::BinLeft {
                    op: *op,
                    left: left.clone(),
                    right: right.clone(),
                });
                let l = self.eval(left)?;
                self.pop_kont();
                self.push_kont(Kont::BinRight {
                    op: *op,
                    left: l.clone(),
                    right: right.clone(),
                });
                let r = self.eval(right)?;
                self.pop_kont();
                binary(*op, l, r)
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.push_kont(Kont::IfAfterCond {
                    condition: condition.clone(),
                    then_value: then_value.clone(),
                    else_value: else_value.clone(),
                });
                let cond = self.eval(condition)?;
                self.pop_kont();
                match cond {
                    CValue::Bool(true) => {
                        self.push_kont(Kont::IfThen {
                            then_value: then_value.clone(),
                        });
                        let value = self.eval(then_value)?;
                        self.pop_kont();
                        Ok(value)
                    }
                    CValue::Bool(false) => {
                        self.push_kont(Kont::IfElse {
                            else_value: else_value.clone(),
                        });
                        let value = self.eval(else_value)?;
                        self.pop_kont();
                        Ok(value)
                    }
                    _ => Err(fault("type", "if condition must be Bool")),
                }
            }
            ExprKind::List(items) => self.eval_seq_items(items, false),
            ExprKind::Tuple(items) => self.eval_seq_items(items, true),
            ExprKind::Record { type_path, fields } => {
                self.finish_record_fields(type_path.clone(), BTreeMap::new(), fields.clone(), None)
            }
            ExprKind::SequenceCons { head, tail } => {
                self.push_kont(Kont::ConsLeft {
                    head: head.clone(),
                    tail: tail.clone(),
                });
                let h = self.eval(head)?;
                self.pop_kont();
                self.push_kont(Kont::ConsAfterHead {
                    head: h.clone(),
                    tail: tail.clone(),
                });
                let t = self.eval(tail)?;
                self.pop_kont();
                cons_values(h, t)
            }
            ExprKind::Index { value, indices } => {
                let seq = self.eval(value)?;
                if !matches!(seq, CValue::Sequence(_)) {
                    return Err(fault("type", "index requires a sequence"));
                }
                let Some(index_expr) = indices.first() else {
                    return Err(fault("invalid_index", "missing index"));
                };
                self.push_kont(Kont::IndexAfterSeq {
                    seq: seq.clone(),
                    index: Box::new(index_expr.clone()),
                });
                let CValue::Int(i) = self.eval(index_expr)? else {
                    self.pop_kont();
                    return Err(fault("type", "index must be Int"));
                };
                self.pop_kont();
                index_seq(seq, i)
            }
            ExprKind::Call { function, args } => self.eval_call(function, args),
            ExprKind::FunctionAbs { param, body, .. } => Ok(CValue::Closure(Box::new(Closure {
                param: param.clone(),
                body: *body.clone(),
                env: self.env.clone(),
                recursive: None,
            }))),
            ExprKind::Recur { name, ty, body, .. } => {
                if !is_fn_type(ty) {
                    return Err(fault(
                        "type",
                        "recur binder type must be a function type A -> B or a record of function types",
                    ));
                }
                if !is_guarded_recur_body(body) {
                    return Err(fault(
                        "unguarded_recursive_binding",
                        "recur body must be a function literal or a record of function literals",
                    ));
                }
                if let ExprKind::FunctionAbs { param, body, .. } = &body.kind {
                    let mut env = self.env.clone();
                    let finished = Closure {
                        param: param.clone(),
                        body: *body.clone(),
                        env: BTreeMap::new(),
                        recursive: Some(name.clone()),
                    };
                    env.insert(name.clone(), CValue::Closure(Box::new(finished.clone())));
                    return Ok(CValue::Closure(Box::new(Closure {
                        param: param.clone(),
                        body: *body.clone(),
                        env,
                        recursive: Some(name.clone()),
                    })));
                }
                Ok(CValue::Closure(Box::new(Closure {
                    param: String::new(),
                    body: *body.clone(),
                    env: self.env.clone(),
                    recursive: Some(name.clone()),
                })))
            }
            ExprKind::Quote { body } => Ok(CValue::Code(Box::new(Code {
                expr: self.mint_binds(body),
            }))),
            ExprKind::QuoteBind {
                param,
                domain,
                body,
            } => Ok(CValue::Code(Box::new(Code {
                expr: self.bind_fresh(param, domain, body),
            }))),
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => self.eval_callable_binder(callee, param, domain, body),
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                let s = match start.as_ref().map(|e| self.eval(e)).transpose()? {
                    Some(CValue::Int(n)) => n,
                    None => 0,
                    Some(other) => {
                        return Err(fault("type", format!("range start must be Int, found {other}")))
                    }
                };
                let e = match end.as_ref().map(|e| self.eval(e)).transpose()? {
                    Some(CValue::Int(n)) => n,
                    None => {
                        return Err(fault(
                            "implementation_unavailable",
                            "unbounded range is not a finite sequence",
                        ));
                    }
                    Some(other) => {
                        return Err(fault("type", format!("range end must be Int, found {other}")))
                    }
                };
                let last = if *inclusive { e } else { e - 1 };
                if last < s {
                    return Ok(CValue::Sequence(Vec::new()));
                }
                Ok(CValue::Sequence(
                    (s..=last).map(CValue::Int).collect(),
                ))
            }
            ExprKind::Cases {
                subject,
                arms,
                else_arm,
            } => {
                if let Some(subject) = subject {
                    self.push_kont(Kont::MatchWaiting {
                        subject: subject.clone(),
                        arms: arms.clone(),
                        else_arm: else_arm.clone(),
                    });
                    let scrutinee = self.eval(subject)?;
                    self.pop_kont();
                    return self.choose_match(scrutinee, arms, else_arm);
                }
                self.eval_cases_arms(arms, else_arm)
            }
            other => Err(fault(
                "implementation_unavailable",
                format!("constructor seam does not evaluate {other:?}"),
            )),
        }
    }

    fn pattern_binds(&mut self, scrutinee: &CValue, pattern: &Expr) -> Result<bool, ConstructorError> {
        match (&pattern.kind, scrutinee) {
            (ExprKind::List(items), CValue::Sequence(values)) if items.is_empty() && values.is_empty() => {
                Ok(true)
            }
            (ExprKind::SequenceCons { head, tail }, CValue::Sequence(values)) if !values.is_empty() => {
                if let ExprKind::Path { segments, .. } = &head.kind {
                    if let Some(name) = segments.first() {
                        self.env.insert(name.clone(), values[0].clone());
                    }
                }
                if let ExprKind::Path { segments, .. } = &tail.kind {
                    if let Some(name) = segments.first() {
                        self.env
                            .insert(name.clone(), CValue::Sequence(values[1..].to_vec()));
                    }
                }
                Ok(true)
            }
            (ExprKind::Path { segments, .. }, _) if segments.len() == 1 => {
                let name = &segments[0];
                if matches!(
                    name.as_str(),
                    "true" | "false" | "partial" | "satisfied" | "unmet"
                ) {
                    return Ok(eq_values(scrutinee, &self.eval(pattern)?));
                }
                self.env.insert(name.clone(), scrutinee.clone());
                Ok(true)
            }
            _ => {
                let cond = self.eval(pattern)?;
                match cond {
                    CValue::Bool(b) => Ok(b),
                    other => Ok(eq_values(&other, scrutinee)),
                }
            }
        }
    }

    fn eval_callable_binder(
        &mut self,
        callee: &Expr,
        param: &str,
        domain: &Expr,
        body: &Expr,
    ) -> Result<CValue, ConstructorError> {
        if let ExprKind::Path { segments, .. } = &callee.kind {
            let name = segments.join(".");
            if name == "quote.view" {
                let fragment = self.eval(domain)?;
                let node = self.quote_view(fragment)?;
                let saved = self.env.clone();
                self.env.insert(param.to_string(), node);
                let result = self.eval(body);
                self.env = saved;
                return result;
            }
            if name == "quote.bind" {
                return Ok(CValue::Code(Box::new(Code {
                    expr: self.bind_fresh(param, domain, body),
                })));
            }
            if name == "quote.open" {
                let package = self.eval(domain)?;
                self.check_fragment_scope(&package)?;
                let opened = open_fragment(package)?;
                let saved = self.env.clone();
                self.env.insert(param.to_string(), opened);
                let result = self.eval(body);
                self.env = saved;
                return result;
            }
            if segments.last().map(String::as_str) == Some("open") {
                return self.open_object(&segments[0], param, domain, body);
            }
        }
        if let ExprKind::Path { segments, .. } = &callee.kind {
            let name = segments.join(".");
            if !self.functions.contains_key(&name) && is_refused_recipe(&name) {
                return Err(fault(
                    "method_unavailable",
                    format!("`{name}` is an ordinary imported function, not a constructor identity"),
                ));
            }
        }
        let cal = self.eval(callee)?;
        let dom = self.eval(domain)?;
        let clos = CValue::Closure(Box::new(Closure {
            param: param.to_string(),
            body: body.clone(),
            env: self.env.clone(),
            recursive: None,
        }));
        self.apply_value(cal, &[dom, clos])
    }

    fn eval_call(&mut self, function: &Expr, args: &[Expr]) -> Result<CValue, ConstructorError> {
        if let ExprKind::Path { segments, .. } = &function.kind {
            if let Some(value) = self.eval_named_call(segments, args)? {
                return Ok(value);
            }
        }
        let cal = self.eval(function)?;
        let mut vals = Vec::new();
        let mut rest: Vec<Expr> = args.to_vec();
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::CallArgs {
                callee: cal.clone(),
                done: vals.clone(),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Box::new(next.clone()),
            });
            vals.push(self.eval(&next)?);
            self.pop_kont();
            self.pop_kont();
        }
        self.apply_value(cal, &vals)
    }

    fn eval_named_call(
        &mut self,
        segments: &[String],
        args: &[Expr],
    ) -> Result<Option<CValue>, ConstructorError> {
        let name = segments.join(".");
        if name.starts_with("quote.") {
            return self.eval_quote_call(&name, args).map(Some);
        }
        if segments.len() == 2 && segments[1] == "pack" {
            return self.pack_object(&segments[0], args).map(Some);
        }
        if name == "transformation_rule_unavailable" || name.ends_with(".opaque") {
            return Err(fault(
                "transformation_rule_unavailable",
                "opaque operation has no exposed transformation rule",
            ));
        }
        if let Some(decl) = self.functions.get(&name).cloned() {
            return self.eval_fn(&name, &decl, args).map(Some);
        }
        if is_refused_recipe(&name) {
            return Err(fault(
                "method_unavailable",
                format!("`{name}` is an ordinary module method, not a constructor operation"),
            ));
        }
        if name == "length" {
            return match self.eval(&args[0])? {
                CValue::Sequence(xs) => Ok(Some(CValue::Int(xs.len() as i128))),
                CValue::Tuple(xs) => Ok(Some(CValue::Int(xs.len() as i128))),
                _ => Err(fault("type", "length expects a sequence")),
            };
        }
        if segments.len() == 1 && args.len() == 1 {
            let recv = self.eval(&args[0])?;
            if let Some(field) = project_field(&recv, &segments[0]) {
                return Ok(Some(field));
            }
        }
        Ok(None)
    }

    fn eval_quote_call(&mut self, name: &str, args: &[Expr]) -> Result<CValue, ConstructorError> {
        match name {
            "quote.evaluate" => {
                let code = self.eval(&args[0])?;
                self.eval_code(code)
            }
            "quote.body" => {
                let code = self.eval(&args[0])?;
                self.quote_body(code)
            }
            "quote.make" => {
                if args.is_empty() {
                    return Err(fault("arity", "quote.make expects a node"));
                }
                let node = self.eval(&args[0])?;
                self.quote_make(node)
            }
            "quote.view" => {
                let code = self.eval(&args[0])?;
                self.quote_view(code)
            }
            "quote.substitute" => {
                if args.len() < 3 {
                    return Err(fault("arity", "quote.substitute expects fragment, reference, replacement"));
                }
                let fragment = self.eval(&args[0])?;
                let reference = match &args[1].kind {
                    ExprKind::Str(name) => name.clone(),
                    ExprKind::Path { segments, .. } if segments.len() == 1 => segments[0].clone(),
                    _ => match self.eval(&args[1])? {
                        CValue::Record { type_name, .. } => type_name,
                        CValue::Code(code) => match &code.expr.kind {
                            ExprKind::Path { segments, .. } => segments.join("."),
                            _ => {
                                return Err(fault(
                                    "type",
                                    "quote.substitute reference must name a binder",
                                ));
                            }
                        },
                        other => {
                            return Err(fault(
                                "type",
                                format!("quote.substitute reference must name a binder, found {other}"),
                            ));
                        }
                    },
                };
                let replacement = self.eval(&args[2])?;
                self.quote_substitute(fragment, &reference, replacement)
            }
            "quote.bind" => {
                if args.is_empty() {
                    return Err(fault("arity", "quote.bind expects a body"));
                }
                let body = match self.eval(&args[0])? {
                    CValue::Code(code) => code.expr,
                    other => rebuild_expr(&other).map_err(|msg| {
                        fault("invalid_code_construction", msg)
                    })?,
                };
                Ok(CValue::Code(Box::new(Code {
                    expr: self.mint_binds(&body),
                })))
            }
            other => Err(fault("unbound", format!("unknown quote operation `{other}`"))),
        }
    }

    fn eval_fn(&mut self, name: &str, decl: &FnDecl, args: &[Expr]) -> Result<CValue, ConstructorError> {
        if decl.inputs.len() != args.len() {
            return Err(fault(
                "arity",
                format!("expected {} arguments", decl.inputs.len()),
            ));
        }
        if self.call_depth >= 256 {
            return Err(fault(
                "stack",
                format!(
                    "call depth exceeded; i={:?} acc={:?} items={:?}",
                    self.env.get("i"),
                    self.env.get("acc"),
                    self.env.get("items").map(|v| match v {
                        CValue::Sequence(xs) => format!("len {}", xs.len()),
                        other => format!("{other}"),
                    })
                ),
            ));
        }
        self.call_depth += 1;
        self.push_frame(name);
        let saved = self.env.clone();
        let result = self.eval_fn_args(name, decl, Vec::new(), args.to_vec());
        let exhausted = matches!(
            &result,
            Err(err) if err.code == "budget_exhausted"
        );
        self.env = saved;
        if !exhausted {
            self.pop_frame();
            self.call_depth = self.call_depth.saturating_sub(1);
        }
        result
    }

    fn eval_fn_args(
        &mut self,
        name: &str,
        decl: &FnDecl,
        mut done: Vec<CValue>,
        mut rest: Vec<Expr>,
    ) -> Result<CValue, ConstructorError> {
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::FnCall {
                name: name.to_string(),
                done: done.clone(),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Box::new(next.clone()),
            });
            done.push(self.eval(&next)?);
            self.pop_kont();
            self.pop_kont();
        }
        self.apply_fn_body(name, decl, &done)
    }

    fn finish_fn_call(
        &mut self,
        name: String,
        mut done: Vec<CValue>,
        rest: Vec<Expr>,
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        let decl = self
            .functions
            .get(&name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
        self.eval_fn_args(&name, &decl, done, rest)
    }

    fn finish_call_args(
        &mut self,
        callee: CValue,
        mut done: Vec<CValue>,
        mut rest: Vec<Expr>,
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::CallArgs {
                callee: callee.clone(),
                done: done.clone(),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Box::new(next.clone()),
            });
            done.push(self.eval(&next)?);
            self.pop_kont();
            self.pop_kont();
        }
        self.apply_value(callee, &done)
    }

    fn apply_fn_body(
        &mut self,
        name: &str,
        decl: &FnDecl,
        vals: &[CValue],
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = self.completed_call(name, vals) {
            return Ok(value);
        }
        for (ty, value) in decl.input_types.iter().zip(vals.iter()) {
            admit_input_type(ty, value)?;
        }
        self.charge()?;
        for (name, value) in decl.inputs.iter().zip(vals.iter()) {
            self.env.insert(name.clone(), value.clone());
        }
        self.refresh_frame();
        let mut last = CValue::Unit;
        for (name, expr) in &decl.defs {
            self.set_frame_next(name);
            last = self.eval(expr)?;
            self.env.insert(name.clone(), last.clone());
        }
        last = self.function_result(decl, name, last)?;
        self.set_frame_next("__done");
        self.remember_call(name, vals, last.clone());
        Ok(last)
    }

    fn eval_seq_items(
        &mut self,
        items: &[Expr],
        as_tuple: bool,
    ) -> Result<CValue, ConstructorError> {
        self.finish_seq_items(as_tuple, Vec::new(), items.to_vec(), None)
    }

    fn finish_seq_items(
        &mut self,
        as_tuple: bool,
        mut done: Vec<CValue>,
        mut rest: Vec<Expr>,
        incoming: Option<CValue>,
    ) -> Result<CValue, ConstructorError> {
        if let Some(value) = incoming {
            done.push(value);
        }
        while !rest.is_empty() {
            let next = rest.remove(0);
            self.push_kont(Kont::SeqItems {
                as_tuple,
                done: done.clone(),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Box::new(next.clone()),
            });
            done.push(self.eval(&next)?);
            self.pop_kont();
            self.pop_kont();
        }
        Ok(if as_tuple {
            CValue::Tuple(done)
        } else {
            CValue::Sequence(done)
        })
    }

    fn finish_record_fields(
        &mut self,
        type_path: Vec<String>,
        mut done: BTreeMap<String, CValue>,
        mut rest: Vec<(String, Expr)>,
        incoming: Option<(String, CValue)>,
    ) -> Result<CValue, ConstructorError> {
        if let Some((name, value)) = incoming {
            done.insert(name, value);
        }
        while !rest.is_empty() {
            let (name, value) = rest.remove(0);
            self.push_kont(Kont::RecordFields {
                type_path: type_path.clone(),
                done: done.clone(),
                current: name.clone(),
                current_expr: Box::new(value.clone()),
                rest: rest.clone(),
            });
            self.push_kont(Kont::EvalExpr {
                expr: Box::new(value.clone()),
            });
            done.insert(name, self.eval(&value)?);
            self.pop_kont();
            self.pop_kont();
        }
        self.finish_record(type_path, done)
    }

    fn finish_record(
        &mut self,
        type_path: Vec<String>,
        map: BTreeMap<String, CValue>,
    ) -> Result<CValue, ConstructorError> {
        let type_name = type_path.join(".");
        if let Some(schema) = self.objects.get(&type_name).cloned() {
            let saved = self.env.clone();
            for (name, value) in &map {
                self.env.insert(name.clone(), value.clone());
            }
            for (name, pred) in &schema.invariants {
                match self.eval(pred)? {
                    CValue::Bool(true) => {}
                    CValue::Bool(false) => {
                        self.env = saved;
                        return Err(fault(
                            "object_invariant_failed",
                            format!("invariant `{name}` failed"),
                        ));
                    }
                    _ => {
                        self.env = saved;
                        return Err(fault(
                            "object_invariant_failed",
                            format!("invariant `{name}` is not Bool"),
                        ));
                    }
                }
            }
            self.env = saved;
            let _ = schema.kind;
        }
        Ok(CValue::Record {
            type_name,
            fields: map,
        })
    }

    fn apply_value(&mut self, callee: CValue, args: &[CValue]) -> Result<CValue, ConstructorError> {
        match callee {
            CValue::Closure(clos) => {
                if args.is_empty() {
                    return Ok(CValue::Closure(clos));
                }
                let frame_name = clos
                    .recursive
                    .clone()
                    .unwrap_or_else(|| format!("function {}", clos.param));
                if let Some(value) = self.completed_closure(&clos, args) {
                    return if args.len() > 1 {
                        self.apply_value(value, &args[1..])
                    } else {
                        Ok(value)
                    };
                }
                self.charge()?;
                if self.call_depth >= 256 {
                    return Err(fault("stack", "call depth exceeded"));
                }
                self.call_depth += 1;
                self.push_frame(&frame_name);
                let saved = self.env.clone();
                self.env.extend(clos.env.clone());
                if let Some(name) = &clos.recursive {
                    self.env
                        .insert(name.clone(), CValue::Closure(clos.clone()));
                }
                if !clos.param.is_empty() {
                    self.env.insert(clos.param.clone(), args[0].clone());
                }
                self.set_frame_next("body");
                self.push_kont(Kont::EvalExpr {
                    expr: Box::new(clos.body.clone()),
                });
                self.refresh_frame();
                let result = self.eval(&clos.body);
                if result.is_ok() {
                    self.pop_kont();
                }
                let exhausted = matches!(
                    &result,
                    Err(err) if err.code == "budget_exhausted"
                );
                self.env = saved;
                if !exhausted {
                    self.pop_frame();
                    self.call_depth = self.call_depth.saturating_sub(1);
                }
                let result = result?;
                self.remember_closure(&clos, args, result.clone());
                if args.len() > 1 {
                    self.apply_value(result, &args[1..])
                } else {
                    Ok(result)
                }
            }
            other => Err(fault(
                "type",
                format!("value is not callable: {other}"),
            )),
        }
    }

    fn eval_code(&mut self, value: CValue) -> Result<CValue, ConstructorError> {
        match value {
            CValue::Code(code) => self.eval(&code.expr),
            other => Ok(other),
        }
    }

    fn quote_body(&self, value: CValue) -> Result<CValue, ConstructorError> {
        let CValue::Code(code) = value else {
            return Ok(body_record("Opaque", None, None));
        };
        if let ExprKind::Path { segments, .. } = &code.expr.kind {
            let name = segments.join(".");
            if let Some(decl) = self.functions.get(&name) {
                if decl.opaque {
                    return Ok(body_record("Opaque", Some(&name), Some("opaque")));
                }
                return Ok(available_body(function_body_expr(decl)));
            }
            return Ok(body_record("Opaque", Some(&name), Some("unbound")));
        }
        Ok(available_body(code.expr.clone()))
    }

    fn quote_make(&mut self, node: CValue) -> Result<CValue, ConstructorError> {
        match node {
            CValue::Code(code) => Ok(CValue::Code(code)),
            CValue::Record { type_name, fields } if type_name == "Fragment" => {
                match fields.get("term") {
                    Some(CValue::Code(code)) => Ok(CValue::Code(code.clone())),
                    Some(other) => match rebuild_expr(other) {
                        Ok(expr) => Ok(CValue::Code(Box::new(Code { expr }))),
                        Err(message) => Err(fault("invalid_code_construction", message)),
                    },
                    None => Err(fault("type", "Fragment missing term")),
                }
            }
            other => match rebuild_expr(&other) {
                Ok(expr) => Ok(CValue::Code(Box::new(Code { expr }))),
                Err(message) => Err(fault("invalid_code_construction", message)),
            },
        }
    }

    fn check_fragment_scope(&self, package: &CValue) -> Result<(), ConstructorError> {
        let CValue::Record { type_name, fields } = package else {
            return Ok(());
        };
        if type_name != "Fragment" {
            return Ok(());
        }
        if let Some(CValue::Record {
            type_name,
            fields: scope,
        }) = fields.get("context")
        {
            if type_name == "Scope" {
                if let Some(CValue::Int(id)) = scope.get("id") {
                    if *id < 0 || !self.scopes.contains(&(*id as u64)) {
                        return Err(fault(
                            "invalid_code_construction",
                            "forged Scope witness",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn quote_view(&mut self, value: CValue) -> Result<CValue, ConstructorError> {
        match fragment_term(value) {
            Ok(expr) => Ok(view_of(
                &expr,
                &self.functions,
                &mut self.next_ref,
                &mut self.scopes,
            )),
            Err(message) => Err(fault("type", message)),
        }
    }

    fn pack_object(&mut self, type_name: &str, args: &[Expr]) -> Result<CValue, ConstructorError> {
        let schema = self
            .objects
            .get(type_name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown object `{type_name}`")))?;
        let mut vals = Vec::new();
        for arg in args {
            vals.push(self.eval(arg)?);
        }
        match schema.kind.as_str() {
            "abstract" => {
                if vals.len() != 1 {
                    return Err(fault("arity", "abstract pack expects one representation"));
                }
                Ok(CValue::Record {
                    type_name: type_name.into(),
                    fields: BTreeMap::from([("repr".into(), vals.remove(0))]),
                })
            }
            "package" => {
                if vals.len() < 2 {
                    return Err(fault("arity", "package pack expects n and data"));
                }
                let n = vals[0].clone();
                let data = vals[1].clone();
                if let (CValue::Int(want), CValue::Sequence(items)) = (&n, &data) {
                    if *want != items.len() as i128 {
                        return Err(fault(
                            "invalid_index",
                            format!("T[n] pack length {} != {want}", items.len()),
                        ));
                    }
                }
                let saved = self.env.clone();
                self.env.insert("n".into(), n.clone());
                self.env.insert("data".into(), data.clone());
                for (inv_name, pred) in &schema.invariants {
                    match self.eval(pred)? {
                        CValue::Bool(true) => {}
                        CValue::Bool(false) => {
                            self.env = saved;
                            return Err(fault(
                                "object_invariant_failed",
                                format!("invariant `{inv_name}` failed"),
                            ));
                        }
                        _ => {
                            self.env = saved;
                            return Err(fault(
                                "object_invariant_failed",
                                format!("invariant `{inv_name}` is not Bool"),
                            ));
                        }
                    }
                }
                self.env = saved;
                Ok(CValue::Record {
                    type_name: type_name.into(),
                    fields: BTreeMap::from([("n".into(), n), ("data".into(), data)]),
                })
            }
            other => Err(fault(
                "type",
                format!("`{type_name}.pack` requires abstract or package representation, found {other}"),
            )),
        }
    }

    fn open_object(
        &mut self,
        type_name: &str,
        param: &str,
        packed_expr: &Expr,
        body: &Expr,
    ) -> Result<CValue, ConstructorError> {
        self.push_kont(Kont::OpenAfterPacked {
            type_name: type_name.to_string(),
            param: param.to_string(),
            packed: Box::new(packed_expr.clone()),
            body: Box::new(body.clone()),
        });
        let packed = self.eval(packed_expr)?;
        self.pop_kont();
        self.finish_open(type_name, param, body, packed)
    }

    fn finish_open(
        &mut self,
        type_name: &str,
        param: &str,
        body: &Expr,
        packed: CValue,
    ) -> Result<CValue, ConstructorError> {
        let CValue::Record {
            type_name: got,
            fields,
        } = &packed
        else {
            return Err(fault("type", "open expects a packed object"));
        };
        if got != type_name {
            return Err(fault(
                "type",
                format!("opened `{got}` as `{type_name}`"),
            ));
        }
        let schema_kind = self
            .objects
            .get(type_name)
            .map(|schema| schema.kind.as_str().to_string())
            .unwrap_or_default();
        let opened = if schema_kind == "abstract" {
            fields
                .get("repr")
                .cloned()
                .ok_or_else(|| fault("type", "abstract pack missing representation"))?
        } else {
            packed.clone()
        };
        let saved = self.env.clone();
        self.env.insert(param.to_string(), opened);
        self.push_kont(Kont::EvalExpr {
            expr: Box::new(body.clone()),
        });
        let result = self.eval(body);
        if result.is_ok() {
            self.pop_kont();
        }
        self.env = saved;
        result
    }

    fn choose_match(
        &mut self,
        scrutinee: CValue,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
    ) -> Result<CValue, ConstructorError> {
        for (cond, value) in arms {
            if self.pattern_binds(&scrutinee, cond)? {
                self.push_kont(Kont::EvalExpr {
                    expr: Box::new(value.clone()),
                });
                let result = self.eval(value)?;
                self.pop_kont();
                return Ok(result);
            }
        }
        self.push_kont(Kont::EvalExpr {
            expr: Box::new(else_arm.clone()),
        });
        let result = self.eval(else_arm)?;
        self.pop_kont();
        Ok(result)
    }

    fn eval_cases_arms(
        &mut self,
        arms: &[(Expr, Expr)],
        else_arm: &Expr,
    ) -> Result<CValue, ConstructorError> {
        let mut rest: Vec<(Expr, Expr)> = arms.to_vec();
        while !rest.is_empty() {
            let (cond, value) = rest.remove(0);
            self.push_kont(Kont::CasesArm {
                cond: Box::new(cond.clone()),
                value: Box::new(value.clone()),
                rest: rest.clone(),
                else_arm: Box::new(else_arm.clone()),
            });
            let cond_value = self.eval(&cond)?;
            self.pop_kont();
            if self.cases_cond_taken(&cond_value)? {
                self.push_kont(Kont::EvalExpr {
                    expr: Box::new(value.clone()),
                });
                let result = self.eval(&value)?;
                self.pop_kont();
                return Ok(result);
            }
        }
        self.push_kont(Kont::EvalExpr {
            expr: Box::new(else_arm.clone()),
        });
        let result = self.eval(else_arm)?;
        self.pop_kont();
        Ok(result)
    }

    fn cases_cond_taken(&self, value: &CValue) -> Result<bool, ConstructorError> {
        match value {
            CValue::Bool(true) => Ok(true),
            CValue::Bool(false) => Ok(false),
            other => {
                if eq_values(other, &CValue::Bool(true)) {
                    Ok(true)
                } else {
                    Err(fault("type", "match condition must be Bool"))
                }
            }
        }
    }

    fn quote_substitute(
        &mut self,
        fragment: CValue,
        reference: &str,
        replacement: CValue,
    ) -> Result<CValue, ConstructorError> {
        let expr = fragment_term(fragment).map_err(|message| fault("type", message))?;
        let expr = substitute_path(&expr, reference, &value_to_expr(&replacement));
        Ok(CValue::Code(Box::new(Code { expr })))
    }

    fn completed_call(&self, name: &str, args: &[CValue]) -> Option<CValue> {
        let key = call_memo_key(name, args)?;
        self.memo.get(&key).cloned()
    }

    fn remember_call(&mut self, name: &str, args: &[CValue], value: CValue) {
        if let Some(key) = call_memo_key(name, args) {
            self.memo.insert(key, value);
        }
    }

    fn completed_closure(&self, clos: &Closure, args: &[CValue]) -> Option<CValue> {
        let key = closure_memo_key(clos, args)?;
        self.memo.get(&key).cloned()
    }

    fn remember_closure(&mut self, clos: &Closure, args: &[CValue], value: CValue) {
        if let Some(key) = closure_memo_key(clos, args) {
            self.memo.insert(key, value);
        }
    }

    fn push_frame(&mut self, function: &str) {
        self.frames.push(ContinuationFrame {
            function: function.to_string(),
            pc: self.visit as u64,
            next: String::new(),
            env: self.env.clone(),
            kont: Vec::new(),
        });
    }

    fn set_frame_next(&mut self, next: &str) {
        if let Some(frame) = self.frames.last_mut() {
            frame.next = next.to_string();
            frame.pc = self.visit as u64;
            frame.env = self.env.clone();
        }
    }

    fn refresh_frame(&mut self) {
        if let Some(frame) = self.frames.last_mut() {
            frame.pc = self.visit as u64;
            frame.env = self.env.clone();
        }
    }

    fn pop_frame(&mut self) {
        self.frames.pop();
    }

    fn snapshot(&self) -> Checkpoint {
        Checkpoint {
            schema: CHECKPOINT_SCHEMA.into(),
            source_id: self.source_id.clone(),
            image: IMAGE_IDENTITY.into(),
            abi: CHECKPOINT_ABI.into(),
            work: self.work,
            remaining: self.work_limit.saturating_sub(self.work),
            accounting: ACCOUNTING_VERSION.into(),
            memo: self.memo.clone(),
            frames: self.frames.clone(),
            scopes: self.scopes.clone(),
            function: self.entry.clone(),
            source: self.source_text.clone(),
            inputs: self.inputs.clone(),
            next_ref: self.next_ref,
        }
    }

    fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), ConstructorError> {
        if checkpoint.schema != CHECKPOINT_SCHEMA || checkpoint.abi != CHECKPOINT_ABI {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint schema or ABI does not match this constructor layer",
            ));
        }
        if !checkpoint.image.is_empty() && checkpoint.image != IMAGE_IDENTITY {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint image identity does not match this constructor layer",
            ));
        }
        if !checkpoint.source_id.is_empty()
            && !self.source_id.is_empty()
            && checkpoint.source_id != self.source_id
        {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint source identity does not match",
            ));
        }
        self.work = checkpoint.work;
        self.memo = checkpoint.memo.clone();
        self.visit = 0;
        self.next_ref = checkpoint.next_ref;
        self.scopes = checkpoint.scopes.clone();
        self.resume_frames = checkpoint.frames.clone();
        self.frames.clear();
        Ok(())
    }

    fn finish_from_stack(&mut self) -> Result<CValue, ConstructorError> {
        let frames = std::mem::take(&mut self.resume_frames);
        if frames.is_empty() {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint has no remaining-work frames",
            ));
        }
        self.frames = frames;
        self.call_depth = u32::try_from(self.frames.len()).unwrap_or(u32::MAX);
        let innermost = self
            .frames
            .last()
            .cloned()
            .ok_or_else(|| fault("incompatible_checkpoint", "checkpoint frame stack is empty"))?;
        self.env = innermost.env.clone();
        let mut value = self.resume_current(&innermost)?;
        while !self.frames.is_empty() {
            self.pop_frame();
            self.call_depth = u32::try_from(self.frames.len()).unwrap_or(0);
            if self.frames.is_empty() {
                return Ok(value);
            }
            let parent = self.frames.last().cloned().ok_or_else(|| {
                fault("incompatible_checkpoint", "checkpoint frame stack is empty")
            })?;
            self.env = parent.env.clone();
            value = self.return_into_kont(value)?;
        }
        Ok(value)
    }

    fn resume_current(&mut self, frame: &ContinuationFrame) -> Result<CValue, ConstructorError> {
        if !frame.kont.is_empty() {
            return self.drain_kont_resume();
        }
        if let Some(decl) = self.functions.get(&frame.function).cloned() {
            let mut started = frame.next.is_empty();
            let mut last = CValue::Unit;
            for (name, expr) in &decl.defs {
                if !started {
                    if let Some(value) = frame.env.get(name) {
                        self.env.insert(name.clone(), value.clone());
                        last = value.clone();
                    }
                    if name == &frame.next {
                        started = true;
                        self.set_frame_next(name);
                        last = self.eval(expr)?;
                        self.env.insert(name.clone(), last.clone());
                    }
                    continue;
                }
                self.set_frame_next(name);
                last = self.eval(expr)?;
                self.env.insert(name.clone(), last.clone());
            }
            return self.function_result(&decl, &frame.function, last);
        }
        if let Some(kont) = frame.kont.first() {
            if let Kont::EvalExpr { expr } = kont.as_ref() {
                return self.eval(expr);
            }
        }
        Err(fault(
            "incompatible_checkpoint",
            format!("frame `{}` has no remaining-work instruction", frame.function),
        ))
    }

    fn drain_kont_resume(&mut self) -> Result<CValue, ConstructorError> {
        let Some(top) = self.pop_kont() else {
            return Err(fault(
                "incompatible_checkpoint",
                "remaining-work continuation is empty",
            ));
        };
        let value = match top.as_ref() {
            Kont::BinLeft { op, left, right } => {
                let left_value = self.eval(left)?;
                self.push_kont(Kont::BinRight {
                    op: *op,
                    left: left_value.clone(),
                    right: right.clone(),
                });
                let right_value = self.eval(right)?;
                self.pop_kont();
                binary(*op, left_value, right_value)?
            }
            Kont::BinRight { op, left, right } => {
                let right_value = self.eval(right)?;
                binary(*op, left.clone(), right_value)?
            }
            Kont::IfAfterCond {
                condition,
                then_value,
                else_value,
            } => match self.eval(condition)? {
                CValue::Bool(true) => {
                    self.push_kont(Kont::IfThen {
                        then_value: then_value.clone(),
                    });
                    let value = self.eval(then_value)?;
                    self.pop_kont();
                    value
                }
                CValue::Bool(false) => {
                    self.push_kont(Kont::IfElse {
                        else_value: else_value.clone(),
                    });
                    let value = self.eval(else_value)?;
                    self.pop_kont();
                    value
                }
                _ => return Err(fault("type", "if condition must be Bool")),
            },
            Kont::IfThen { then_value } => self.eval(then_value)?,
            Kont::IfElse { else_value } => self.eval(else_value)?,
            Kont::CallArgs {
                callee,
                done,
                rest,
            } => self.finish_call_args(callee.clone(), done.clone(), rest.clone(), None)?,
            Kont::FnCall { name, done, rest } => {
                self.finish_fn_call(name.clone(), done.clone(), rest.clone(), None)?
            }
            Kont::SeqItems {
                as_tuple,
                done,
                rest,
            } => self.finish_seq_items(*as_tuple, done.clone(), rest.clone(), None)?,
            Kont::RecordFields {
                type_path,
                done,
                current,
                current_expr,
                rest,
            } => {
                let value = self.eval(current_expr)?;
                self.finish_record_fields(
                    type_path.clone(),
                    done.clone(),
                    rest.clone(),
                    Some((current.clone(), value)),
                )?
            }
            Kont::IndexAfterSeq { seq, index } => {
                let CValue::Int(i) = self.eval(index)? else {
                    return Err(fault("type", "index must be Int"));
                };
                index_seq(seq.clone(), i)?
            }
            Kont::UnaryAfter { op, value } => apply_unary(*op, self.eval(value)?)?,
            Kont::ConsLeft { head, tail } => {
                let h = self.eval(head)?;
                self.push_kont(Kont::ConsAfterHead {
                    head: h.clone(),
                    tail: tail.clone(),
                });
                let t = self.eval(tail)?;
                self.pop_kont();
                cons_values(h, t)?
            }
            Kont::ConsAfterHead { head, tail } => cons_values(head.clone(), self.eval(tail)?)?,
            Kont::MatchWaiting {
                subject,
                arms,
                else_arm,
            } => {
                let scrutinee = self.eval(subject)?;
                self.choose_match(scrutinee, arms, else_arm)?
            }
            Kont::CasesArm {
                cond,
                value,
                rest,
                else_arm,
            } => {
                let cond_value = self.eval(cond)?;
                if self.cases_cond_taken(&cond_value)? {
                    self.eval(value)?
                } else {
                    self.eval_cases_arms(rest, else_arm)?
                }
            }
            Kont::OpenAfterPacked {
                type_name,
                param,
                packed,
                body,
            } => {
                let value = self.eval(packed)?;
                self.finish_open(type_name, param, body, value)?
            }
            Kont::EvalExpr { expr } => self.eval(expr)?,
        };
        self.return_into_kont(value)
    }

    fn return_into_kont(&mut self, incoming: CValue) -> Result<CValue, ConstructorError> {
        let Some(top) = self.pop_kont() else {
            return self.finish_after_value(incoming);
        };
        let value = match top.as_ref() {
            Kont::BinLeft { op, right, .. } => {
                self.push_kont(Kont::BinRight {
                    op: *op,
                    left: incoming.clone(),
                    right: right.clone(),
                });
                let right_value = self.eval(right)?;
                self.pop_kont();
                binary(*op, incoming, right_value)?
            }
            Kont::BinRight { op, left, .. } => binary(*op, left.clone(), incoming)?,
            Kont::IfAfterCond {
                then_value,
                else_value,
                ..
            } => match incoming {
                CValue::Bool(true) => {
                    self.push_kont(Kont::IfThen {
                        then_value: then_value.clone(),
                    });
                    let value = self.eval(then_value)?;
                    self.pop_kont();
                    value
                }
                CValue::Bool(false) => {
                    self.push_kont(Kont::IfElse {
                        else_value: else_value.clone(),
                    });
                    let value = self.eval(else_value)?;
                    self.pop_kont();
                    value
                }
                _ => return Err(fault("type", "if condition must be Bool")),
            },
            Kont::IfThen { .. } | Kont::IfElse { .. } | Kont::EvalExpr { .. } => incoming,
            Kont::CallArgs {
                callee,
                done,
                rest,
            } => self.finish_call_args(callee.clone(), done.clone(), rest.clone(), Some(incoming))?,
            Kont::FnCall { name, done, rest } => {
                self.finish_fn_call(name.clone(), done.clone(), rest.clone(), Some(incoming))?
            }
            Kont::SeqItems {
                as_tuple,
                done,
                rest,
            } => self.finish_seq_items(*as_tuple, done.clone(), rest.clone(), Some(incoming))?,
            Kont::RecordFields {
                type_path,
                done,
                current,
                rest,
                ..
            } => self.finish_record_fields(
                type_path.clone(),
                done.clone(),
                rest.clone(),
                Some((current.clone(), incoming)),
            )?,
            Kont::IndexAfterSeq { seq, .. } => match incoming {
                CValue::Int(i) => index_seq(seq.clone(), i)?,
                _ => return Err(fault("type", "index must be Int")),
            },
            Kont::UnaryAfter { op, .. } => apply_unary(*op, incoming)?,
            Kont::ConsLeft { tail, .. } => {
                self.push_kont(Kont::ConsAfterHead {
                    head: incoming.clone(),
                    tail: tail.clone(),
                });
                let t = self.eval(tail)?;
                self.pop_kont();
                cons_values(incoming, t)?
            }
            Kont::ConsAfterHead { head, .. } => cons_values(head.clone(), incoming)?,
            Kont::MatchWaiting { arms, else_arm, .. } => {
                self.choose_match(incoming, arms, else_arm)?
            }
            Kont::CasesArm {
                value,
                rest,
                else_arm,
                ..
            } => {
                if self.cases_cond_taken(&incoming)? {
                    self.eval(value)?
                } else {
                    self.eval_cases_arms(rest, else_arm)?
                }
            }
            Kont::OpenAfterPacked {
                type_name,
                param,
                body,
                ..
            } => self.finish_open(type_name, param, body, incoming)?,
        };
        self.return_into_kont(value)
    }

    fn finish_after_value(&mut self, incoming: CValue) -> Result<CValue, ConstructorError> {
        let Some(frame) = self.frames.last().cloned() else {
            return Ok(incoming);
        };
        let Some(decl) = self.functions.get(&frame.function).cloned() else {
            return Ok(incoming);
        };
        if frame.next == "__done" {
            return Ok(incoming);
        }
        let mut last = incoming;
        if !frame.next.is_empty() && frame.next != "body" {
            self.env.insert(frame.next.clone(), last.clone());
        }
        let mut seen = frame.next.is_empty() || frame.next == "body";
        for (name, expr) in &decl.defs {
            if !seen {
                if name == &frame.next {
                    seen = true;
                }
                continue;
            }
            self.set_frame_next(name);
            last = self.eval(expr)?;
            self.env.insert(name.clone(), last.clone());
        }
        self.function_result(&decl, &frame.function, last)
    }

    fn function_result(
        &self,
        decl: &FnDecl,
        name: &str,
        last: CValue,
    ) -> Result<CValue, ConstructorError> {
        if decl.outputs.len() > 1 {
            let mut fields = BTreeMap::new();
            for output in &decl.outputs {
                let value = self.env.get(output).cloned().ok_or_else(|| {
                    fault("unbound", format!("missing output `{output}`"))
                })?;
                fields.insert(output.clone(), value);
            }
            return Ok(CValue::Record {
                type_name: name.to_string(),
                fields,
            });
        }
        if let Some(output) = &decl.output {
            return self
                .env
                .get(output)
                .cloned()
                .ok_or_else(|| fault("unbound", format!("missing output `{output}`")));
        }
        Ok(last)
    }

    fn fresh_token(&mut self, spelling: &str) -> String {
        self.next_ref += 1;
        format!("#{spelling}.{}", self.next_ref)
    }

    fn bind_fresh(&mut self, param: &str, domain: &Expr, body: &Expr) -> Expr {
        let token = self.fresh_token(param);
        let domain = self.mint_binds(domain);
        let body = substitute_path(&self.mint_binds(body), param, &path_expr(&token));
        dummy_expr(ExprKind::FunctionAbs {
            param: token,
            domain: Box::new(domain),
            body: Box::new(body),
        })
    }

    fn mint_binds(&mut self, expr: &Expr) -> Expr {
        let kind = match &expr.kind {
            ExprKind::QuoteBind {
                param,
                domain,
                body,
            } => return self.bind_fresh(param, domain, body),
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } if matches!(
                &callee.kind,
                ExprKind::Path { segments, .. } if segments.join(".") == "quote.bind"
            ) =>
            {
                return self.bind_fresh(param, domain, body);
            }
            ExprKind::FunctionAbs {
                param,
                domain,
                body,
            } => ExprKind::FunctionAbs {
                param: param.clone(),
                domain: Box::new(self.mint_binds(domain)),
                body: Box::new(self.mint_binds(body)),
            },
            ExprKind::Quote { body } => ExprKind::Quote {
                body: Box::new(self.mint_binds(body)),
            },
            ExprKind::Call { function, args } => ExprKind::Call {
                function: Box::new(self.mint_binds(function)),
                args: args.iter().map(|arg| self.mint_binds(arg)).collect(),
            },
            ExprKind::Binary { op, left, right } => ExprKind::Binary {
                op: *op,
                left: Box::new(self.mint_binds(left)),
                right: Box::new(self.mint_binds(right)),
            },
            ExprKind::Unary { op, value } => ExprKind::Unary {
                op: *op,
                value: Box::new(self.mint_binds(value)),
            },
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => ExprKind::If {
                condition: Box::new(self.mint_binds(condition)),
                then_value: Box::new(self.mint_binds(then_value)),
                else_value: Box::new(self.mint_binds(else_value)),
            },
            ExprKind::List(items) => {
                ExprKind::List(items.iter().map(|item| self.mint_binds(item)).collect())
            }
            ExprKind::Tuple(items) => {
                ExprKind::Tuple(items.iter().map(|item| self.mint_binds(item)).collect())
            },
            ExprKind::Cases {
                subject,
                arms,
                else_arm,
            } => ExprKind::Cases {
                subject: subject.as_ref().map(|subject| Box::new(self.mint_binds(subject))),
                arms: arms
                    .iter()
                    .map(|(cond, value)| (self.mint_binds(cond), self.mint_binds(value)))
                    .collect(),
                else_arm: Box::new(self.mint_binds(else_arm)),
            },
            ExprKind::Recur { name, ty, body } => ExprKind::Recur {
                name: name.clone(),
                ty: Box::new(self.mint_binds(ty)),
                body: Box::new(self.mint_binds(body)),
            },
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => ExprKind::CallableBinder {
                callee: Box::new(self.mint_binds(callee)),
                param: param.clone(),
                domain: Box::new(self.mint_binds(domain)),
                body: Box::new(self.mint_binds(body)),
            },
            other => other.clone(),
        };
        Expr {
            kind,
            source: expr.source,
        }
    }

    fn run_query(&mut self, name: &str, inputs: &BTreeMap<String, CValue>) -> Result<Receipt, ConstructorError> {
        let decl = self
            .queries
            .get(name)
            .cloned()
            .ok_or_else(|| fault("unbound", format!("unknown query `{name}`")))?;
        let saved = self.env.clone();
        for (key, value) in inputs {
            self.env.insert(key.clone(), value.clone());
        }
        for (name, expr) in &decl.defs {
            let value = self.eval(expr)?;
            self.env.insert(name.clone(), value);
        }
        let receipt = if decl.form == "code" {
            Receipt {
                execution: "returned".into(),
                fulfillment: "satisfied".into(),
                representation: "code".into(),
                payload: Some("code".into()),
                evidence: vec!["direct evaluation of requested code form".into()],
                remaining: Vec::new(),
                diagnostic_code: None,
            }
        } else if decl.method.as_deref().is_none_or(|method| {
            method == "missing"
                || method == "method_unavailable"
                || !self.functions.contains_key(method)
        }) {
            let target = self
                .env
                .get("target")
                .cloned()
                .or_else(|| self.env.get("question").cloned());
            match target {
                Some(CValue::Code(code)) if decl.form != "value" => match self.eval(&code.expr) {
                    Ok(value) => Receipt {
                        execution: "returned".into(),
                        fulfillment: "satisfied".into(),
                        representation: representation_of(&value),
                        payload: Some(value.to_string()),
                        evidence: vec!["direct evaluation".into()],
                        remaining: Vec::new(),
                        diagnostic_code: None,
                    },
                    Err(err) if err.code == "budget_exhausted" => Receipt {
                        execution: "suspended".into(),
                        fulfillment: "partial".into(),
                        representation: "absent".into(),
                        payload: None,
                        evidence: Vec::new(),
                        remaining: vec!["work budget".into()],
                        diagnostic_code: Some(err.code),
                    },
                    Err(err) => Receipt {
                        execution: "faulted".into(),
                        fulfillment: "unmet".into(),
                        representation: "absent".into(),
                        payload: None,
                        evidence: Vec::new(),
                        remaining: vec![err.message],
                        diagnostic_code: Some(err.code),
                    },
                },
                Some(_) => Receipt {
                    execution: "returned".into(),
                    fulfillment: "unmet".into(),
                    representation: "code".into(),
                    payload: Some("retained".into()),
                    evidence: Vec::new(),
                    remaining: vec!["method_unavailable".into()],
                    diagnostic_code: Some("method_unavailable".into()),
                },
                None => Receipt {
                    execution: "returned".into(),
                    fulfillment: "unmet".into(),
                    representation: "absent".into(),
                    payload: None,
                    evidence: Vec::new(),
                    remaining: vec!["method_unavailable".into()],
                    diagnostic_code: Some("method_unavailable".into()),
                },
            }
        } else if decl.accept.as_deref() == Some("unchecked") {
            let payload = self.env.get("payload").cloned().or_else(|| {
                self.env
                    .values()
                    .next()
                    .cloned()
            });
            Receipt {
                execution: "returned".into(),
                fulfillment: if payload.is_some() {
                    "partial"
                } else {
                    "unmet"
                }
                .into(),
                representation: payload
                    .as_ref()
                    .map(representation_of)
                    .unwrap_or_else(|| "absent".into()),
                payload: payload.map(|v| v.to_string()),
                evidence: vec!["unchecked candidate".into()],
                remaining: vec!["global bound missing".into()],
                diagnostic_code: None,
            }
        } else {
            let payload = self.env.get("result").cloned();
            Receipt {
                execution: "returned".into(),
                fulfillment: if payload.is_some() {
                    "satisfied"
                } else {
                    "unmet"
                }
                .into(),
                representation: payload
                    .as_ref()
                    .map(representation_of)
                    .unwrap_or_else(|| "absent".into()),
                payload: payload.map(|v| v.to_string()),
                evidence: vec!["authored method".into()],
                remaining: Vec::new(),
                diagnostic_code: None,
            }
        };
        self.env = saved;
        Ok(receipt)
    }
}

impl CValue {
    fn canon(self) -> Self {
        match self {
            Self::Rat { num, den } => {
                let g = gcd(num.abs(), den.abs());
                let sign = if den < 0 { -1 } else { 1 };
                Self::Rat {
                    num: sign * num / g,
                    den: den.abs() / g,
                }
            }
            other => other,
        }
    }
}

fn representation_of(value: &CValue) -> String {
    match value {
        CValue::Int(_) | CValue::Rat { .. } | CValue::Bool(_) => "exact_scalar".into(),
        CValue::Float64(_) => "rounded_scalar".into(),
        CValue::Code(_) => "code".into(),
        CValue::Absent => "absent".into(),
        _ => "structured".into(),
    }
}

fn fault(code: &str, message: impl Into<String>) -> ConstructorError {
    ConstructorError {
        code: code.into(),
        message: message.into(),
    }
}

fn parse_int(text: &str) -> Result<CValue, ConstructorError> {
    parse_i128(text).map(CValue::Int)
}

fn parse_i128(text: &str) -> Result<i128, ConstructorError> {
    text.replace('_', "")
        .parse::<i128>()
        .map_err(|_| fault("invalid_literal", format!("not Int: {text}")))
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    if a == 0 { 1 } else { a.abs() }
}

fn as_rat(value: CValue) -> Result<(i128, i128), ConstructorError> {
    match value {
        CValue::Int(n) => Ok((n, 1)),
        CValue::Rat { num, den } => Ok((num, den)),
        CValue::Bool(b) => Ok((i128::from(b), 1)),
        other => Err(fault("type", format!("expected exact scalar, found {other}"))),
    }
}

fn neg(value: CValue) -> Result<CValue, ConstructorError> {
    match value {
        CValue::Int(n) => Ok(CValue::Int(-n)),
        CValue::Rat { num, den } => Ok(CValue::Rat { num: -num, den }.canon()),
        CValue::Float64(x) => Ok(CValue::Float64(-x)),
        other => Err(fault("type", format!("cannot negate {other}"))),
    }
}

fn apply_unary(op: UnaryOp, value: CValue) -> Result<CValue, ConstructorError> {
    match op {
        UnaryOp::Neg => neg(value),
        UnaryOp::Pos => Ok(value),
        UnaryOp::Not => match value {
            CValue::Bool(b) => Ok(CValue::Bool(!b)),
            _ => Err(fault("type", "not expects Bool")),
        },
    }
}

fn index_seq(seq: CValue, i: i128) -> Result<CValue, ConstructorError> {
    let CValue::Sequence(items) = seq else {
        return Err(fault("type", "index requires a sequence"));
    };
    if i < 0 || i as usize >= items.len() {
        return Err(fault("invalid_index", "sequence index out of range"));
    }
    Ok(items[i as usize].clone())
}

fn cons_values(head: CValue, tail: CValue) -> Result<CValue, ConstructorError> {
    match tail {
        CValue::Sequence(mut rest) => {
            rest.insert(0, head);
            Ok(CValue::Sequence(rest))
        }
        other => Err(fault(
            "type",
            format!("sequence tail must be a sequence, found {other}"),
        )),
    }
}

fn binary(op: BinaryOp, left: CValue, right: CValue) -> Result<CValue, ConstructorError> {
    if matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
    ) && (matches!(left, CValue::Float64(_)) || matches!(right, CValue::Float64(_)))
    {
        let l = float_of(&left)?;
        let r = float_of(&right)?;
        let out = match op {
            BinaryOp::Add => l + r,
            BinaryOp::Sub => l - r,
            BinaryOp::Mul => l * r,
            BinaryOp::Div => {
                if r == 0.0 {
                    return Err(fault("division_by_zero", "Float64 division by zero"));
                }
                l / r
            }
            _ => unreachable!(),
        };
        if !out.is_finite() {
            return Err(fault("non_finite_scalar", "non-finite Float64 result"));
        }
        return Ok(CValue::Float64(out));
    }
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
            let (ln, ld) = as_rat(left)?;
            let (rn, rd) = as_rat(right)?;
            let (num, den) = match op {
                BinaryOp::Add => (ln * rd + rn * ld, ld * rd),
                BinaryOp::Sub => (ln * rd - rn * ld, ld * rd),
                BinaryOp::Mul => (ln * rn, ld * rd),
                BinaryOp::Div => {
                    if rn == 0 {
                        return Err(fault("division_by_zero", "exact division by zero"));
                    }
                    (ln * rd, ld * rn)
                }
                _ => unreachable!(),
            };
            if den == 0 {
                return Err(fault("division_by_zero", "exact division by zero"));
            }
            let value = CValue::Rat { num, den }.canon();
            if let CValue::Rat { num, den } = &value {
                if *den == 1 && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
                    return Ok(CValue::Int(*num));
                }
            }
            Ok(value)
        }
        BinaryOp::Eq => Ok(CValue::Bool(eq_values(&left, &right))),
        BinaryOp::Ne => Ok(CValue::Bool(!eq_values(&left, &right))),
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            let (ln, ld) = as_rat(left)?;
            let (rn, rd) = as_rat(right)?;
            let cmp = (ln * rd).cmp(&(rn * ld));
            Ok(CValue::Bool(match op {
                BinaryOp::Lt => cmp.is_lt(),
                BinaryOp::Le => cmp.is_le(),
                BinaryOp::Gt => cmp.is_gt(),
                BinaryOp::Ge => cmp.is_ge(),
                _ => false,
            }))
        }
        BinaryOp::And => match (left, right) {
            (CValue::Bool(a), CValue::Bool(b)) => Ok(CValue::Bool(a && b)),
            _ => Err(fault("type", "and expects Bool")),
        },
        BinaryOp::Or => match (left, right) {
            (CValue::Bool(a), CValue::Bool(b)) => Ok(CValue::Bool(a && b || a || b)),
            _ => Err(fault("type", "or expects Bool")),
        },
        BinaryOp::In => match right {
            CValue::Sequence(xs) => Ok(CValue::Bool(xs.iter().any(|item| eq_values(item, &left)))),
            _ => Err(fault("type", "`in` expects a sequence")),
        },
        other => Err(fault(
            "implementation_unavailable",
            format!("operator {other:?} is not a scalar carrier operation"),
        )),
    }
}

fn float_of(value: &CValue) -> Result<f64, ConstructorError> {
    match value {
        CValue::Float64(x) => Ok(*x),
        CValue::Int(n) => Ok(*n as f64),
        CValue::Rat { num, den } => Ok(*num as f64 / *den as f64),
        other => Err(fault("type", format!("not Float64: {other}"))),
    }
}

fn eq_values(left: &CValue, right: &CValue) -> bool {
    match (left, right) {
        (CValue::Int(a), CValue::Int(b)) => a == b,
        (CValue::Bool(a), CValue::Bool(b)) => a == b,
        (CValue::Float64(a), CValue::Float64(b)) => *a == *b,
        (CValue::Rat { num: an, den: ad }, CValue::Rat { num: bn, den: bd }) => an * bd == bn * ad,
        (CValue::Int(a), CValue::Rat { num, den }) | (CValue::Rat { num, den }, CValue::Int(a)) => {
            a * den == *num
        }
        (CValue::Sequence(a), CValue::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| eq_values(x, y))
        }
        (CValue::Tuple(a), CValue::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| eq_values(x, y))
        }
        _ => left == right,
    }
}

fn tuple_index(name: &str) -> Option<usize> {
    name.strip_prefix('_')?.parse().ok()
}

fn value_to_expr(value: &CValue) -> Expr {
    let kind = match value {
        CValue::Bool(v) => ExprKind::Bool(*v),
        CValue::Int(v) => ExprKind::Int(v.to_string()),
        CValue::Rat { num, den } => ExprKind::Rational {
            numer: num.to_string(),
            denom: den.to_string(),
        },
        CValue::Float64(v) => ExprKind::Float(format!("{v}f64")),
        CValue::Sequence(items) => ExprKind::List(items.iter().map(value_to_expr).collect()),
        CValue::Tuple(items) => ExprKind::Tuple(items.iter().map(value_to_expr).collect()),
        CValue::Code(code) => code.expr.kind.clone(),
        CValue::Record { type_name, fields } => ExprKind::Record {
            type_path: vec![type_name.clone()],
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_expr(v)))
                .collect(),
        },
        other => ExprKind::Path {
            segments: vec![other.to_string()],
            generics: None,
        },
    };
    Expr {
        kind,
        source: emath_core::Span::default(),
    }
}

fn substitute_path(expr: &Expr, name: &str, replacement: &Expr) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Path { segments, .. } if segments.join(".") == name => replacement.kind.clone(),
        ExprKind::Binary { op, left, right } => ExprKind::Binary {
            op: *op,
            left: Box::new(substitute_path(left, name, replacement)),
            right: Box::new(substitute_path(right, name, replacement)),
        },
        ExprKind::Unary { op, value } => ExprKind::Unary {
            op: *op,
            value: Box::new(substitute_path(value, name, replacement)),
        },
        ExprKind::Call { function, args } => ExprKind::Call {
            function: Box::new(substitute_path(function, name, replacement)),
            args: args
                .iter()
                .map(|arg| substitute_path(arg, name, replacement))
                .collect(),
        },
        ExprKind::FunctionAbs { param, domain, body } => {
            if param == name {
                substitute_path(body, name, replacement).kind
            } else {
                ExprKind::FunctionAbs {
                    param: param.clone(),
                    domain: Box::new(substitute_path(domain, name, replacement)),
                    body: Box::new(substitute_path(body, name, replacement)),
                }
            }
        }
        ExprKind::Recur {
            name: recur_name,
            ty,
            body,
        } => {
            if recur_name == name {
                substitute_path(body, name, replacement).kind
            } else {
                ExprKind::Recur {
                    name: recur_name.clone(),
                    ty: Box::new(substitute_path(ty, name, replacement)),
                    body: Box::new(substitute_path(body, name, replacement)),
                }
            }
        }
        ExprKind::QuoteBind { param, domain, body } => {
            if param == name {
                substitute_path(body, name, replacement).kind
            } else {
                ExprKind::QuoteBind {
                    param: param.clone(),
                    domain: Box::new(substitute_path(domain, name, replacement)),
                    body: Box::new(substitute_path(body, name, replacement)),
                }
            }
        }
        ExprKind::CallableBinder {
            callee,
            param,
            domain,
            body,
        } => {
            if param == name {
                substitute_path(body, name, replacement).kind
            } else {
                ExprKind::CallableBinder {
                    callee: Box::new(substitute_path(callee, name, replacement)),
                    param: param.clone(),
                    domain: Box::new(substitute_path(domain, name, replacement)),
                    body: Box::new(substitute_path(body, name, replacement)),
                }
            }
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => ExprKind::If {
            condition: Box::new(substitute_path(condition, name, replacement)),
            then_value: Box::new(substitute_path(then_value, name, replacement)),
            else_value: Box::new(substitute_path(else_value, name, replacement)),
        },
        ExprKind::Quote { body } => ExprKind::Quote {
            body: Box::new(substitute_path(body, name, replacement)),
        },
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => ExprKind::Cases {
            subject: subject
                .as_ref()
                .map(|subject| Box::new(substitute_path(subject, name, replacement))),
            arms: arms
                .iter()
                .map(|(cond, value)| {
                    (
                        substitute_path(cond, name, replacement),
                        substitute_path(value, name, replacement),
                    )
                })
                .collect(),
            else_arm: Box::new(substitute_path(else_arm, name, replacement)),
        },
        ExprKind::List(items) => ExprKind::List(
            items
                .iter()
                .map(|item| substitute_path(item, name, replacement))
                .collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items
                .iter()
                .map(|item| substitute_path(item, name, replacement))
                .collect(),
        ),
        other => other.clone(),
    };
    Expr {
        kind,
        source: expr.source,
    }
}

fn dummy_expr(kind: ExprKind) -> Expr {
    Expr {
        kind,
        source: emath_core::Span::default(),
    }
}

fn path_expr(name: &str) -> Expr {
    dummy_expr(ExprKind::Path {
        segments: vec![name.into()],
        generics: None,
    })
}

fn is_refused_recipe(name: &str) -> bool {
    matches!(
        name,
        "derivative"
            | "jacobian"
            | "solve"
            | "minimize"
            | "maximize"
            | "partial"
            | "total"
            | "sum"
            | "product"
            | "forall"
            | "exists"
            | "integral"
            | "series"
            | "limit"
            | "sample_limit"
            | "einsum"
            | "series_from_csv"
            | "rat"
            | "rat_add"
            | "rat_norm"
            | "euler_maruyama"
            | "stratonovich"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "sinh"
            | "cosh"
            | "tanh"
            | "exp"
            | "ln"
            | "log"
            | "log2"
            | "log10"
            | "sqrt"
            | "cbrt"
            | "hypot"
            | "pow"
            | "abs"
            | "floor"
            | "ceil"
            | "sign"
            | "recip"
            | "is_finite"
            | "gamma"
            | "lgamma"
            | "beta"
            | "lbeta"
            | "erf"
            | "erfc"
            | "gamma_error_bound"
            | "min"
            | "max"
            | "mod"
            | "generating_function"
            | "coefficient"
            | "dot"
            | "norm"
            | "cross"
            | "transpose"
            | "det"
            | "inv"
            | "matmul"
            | "reachability"
            | "shortest_path"
            | "connected_components"
            | "rk4"
            | "euler"
            | "integrate"
            | "quad"
            | "factorial"
            | "binomial"
            | "choose"
            | "vec_add"
            | "plot"
    )
}

fn is_fn_ctor(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Path { segments, .. } if segments.last().map(String::as_str) == Some("Fn")
    )
}

fn is_fn_type(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Call { function, args } => is_fn_ctor(function) && args.len() == 2,
        ExprKind::Record { fields, .. } => {
            !fields.is_empty() && fields.iter().all(|(_, ty)| is_fn_type(ty))
        }
        _ => false,
    }
}

fn is_guarded_recur_body(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionAbs { .. } => true,
        ExprKind::Record { fields, .. } => {
            !fields.is_empty() && fields.iter().all(|(_, body)| is_guarded_recur_body(body))
        }
        _ => false,
    }
}

fn schema_tag(name: &str) -> CValue {
    CValue::Record {
        type_name: name.into(),
        fields: BTreeMap::new(),
    }
}

fn is_schema_tag(name: &str) -> bool {
    matches!(
        name,
        "partial"
            | "satisfied"
            | "unmet"
            | "returned"
            | "suspended"
            | "faulted"
            | "code"
            | "value"
            | "exact_scalar"
            | "method_unavailable"
            | "zero_normalizer"
            | "Literal"
            | "Local"
            | "Global"
            | "Call"
            | "Closure"
            | "Sequence"
            | "Opaque"
            | "Available"
            | "Add"
            | "Sub"
            | "Mul"
            | "Div"
            | "Eq"
            | "Ne"
            | "Lt"
            | "Le"
            | "Gt"
            | "Ge"
            | "Pow"
            | "Imply"
            | "Iff"
            | "Asymp"
            | "In"
            | "transparent"
            | "opaque"
            | "Match"
            | "Cases"
            | "Quotation"
            | "Fragment"
            | "Scope"
            | "Branch"
            | "Index"
            | "Record"
            | "Recur"
            | "Projection"
        )
}

fn body_record(kind: &str, identity: Option<&str>, signature: Option<&str>) -> CValue {
    let mut fields = BTreeMap::from([("kind".into(), schema_tag(kind))]);
    if let Some(identity) = identity {
        fields.insert("identity".into(), schema_tag(identity));
    }
    if let Some(signature) = signature {
        fields.insert("signature".into(), schema_tag(signature));
    }
    CValue::Record {
        type_name: kind.into(),
        fields,
    }
}

fn available_body(expr: Expr) -> CValue {
    CValue::Record {
        type_name: "Available".into(),
        fields: BTreeMap::from([
            ("kind".into(), schema_tag("Available")),
            ("fragment".into(), CValue::Code(Box::new(Code { expr }))),
        ]),
    }
}

fn code_of(expr: Expr) -> CValue {
    CValue::Code(Box::new(Code { expr }))
}

fn mint_scope(next_ref: &mut u64, scopes: &mut BTreeSet<u64>, binder: Option<&str>) -> CValue {
    *next_ref += 1;
    let id = *next_ref;
    scopes.insert(id);
    CValue::Record {
        type_name: "Scope".into(),
        fields: BTreeMap::from([
            ("id".into(), CValue::Int(id as i128)),
            ("binder".into(), schema_tag(binder.unwrap_or("closed"))),
            ("token".into(), schema_tag(&format!("#scope.{id}"))),
        ]),
    }
}

fn fragment_package(term: Expr, scope: CValue) -> CValue {
    CValue::Record {
        type_name: "Fragment".into(),
        fields: BTreeMap::from([
            ("term".into(), code_of(term)),
            ("context".into(), scope),
        ]),
    }
}

fn fragment_term(value: CValue) -> Result<Expr, String> {
    match value {
        CValue::Code(code) => Ok(code.expr),
        CValue::Record { type_name, fields }
            if type_name == "Fragment" || fields.contains_key("term") =>
        {
            match fields.get("term") {
                Some(CValue::Code(code)) => Ok(code.expr.clone()),
                Some(other) => rebuild_expr(other),
                None => Err("Fragment missing term".into()),
            }
        }
        _ => Err("quote expects Code or Fragment".into()),
    }
}

fn open_fragment(package: CValue) -> Result<CValue, ConstructorError> {
    match fragment_term(package) {
        Ok(expr) => Ok(CValue::Code(Box::new(Code { expr }))),
        Err(message) => Err(fault("type", message)),
    }
}

fn view_record(tag: &str, fields: BTreeMap<String, CValue>) -> CValue {
    let mut fields = fields;
    fields.insert("kind".into(), schema_tag(tag));
    CValue::Record {
        type_name: tag.into(),
        fields,
    }
}

fn view_of(
    expr: &Expr,
    functions: &BTreeMap<String, FnDecl>,
    next_ref: &mut u64,
    scopes: &mut BTreeSet<u64>,
) -> CValue {
    let mut pack = |term: Expr, binder: Option<&str>| {
        fragment_package(term, mint_scope(next_ref, scopes, binder))
    };
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => view_record(
            "Literal",
            BTreeMap::from([("value".into(), parse_int(text).unwrap_or(CValue::Int(0)))]),
        ),
        ExprKind::Rational { numer, denom } => {
            let value = match (parse_i128(numer), parse_i128(denom)) {
                (Ok(num), Ok(den)) if den != 0 => CValue::Rat { num, den: den.abs() }.canon(),
                _ => CValue::Int(0),
            };
            view_record("Literal", BTreeMap::from([("value".into(), value)]))
        }
        ExprKind::Bool(value) => view_record(
            "Literal",
            BTreeMap::from([("value".into(), CValue::Bool(*value))]),
        ),
        ExprKind::Path { segments, .. } => {
            let name = segments.join(".");
            if functions.contains_key(&name) {
                let visibility = if functions[&name].opaque {
                    "opaque"
                } else {
                    "transparent"
                };
                view_record(
                    "Global",
                    BTreeMap::from([
                        ("name".into(), schema_tag(&name)),
                        ("visibility".into(), schema_tag(visibility)),
                    ]),
                )
            } else if segments.len() >= 2 {
                let field = segments[segments.len() - 1].as_str();
                let container = segments[..segments.len() - 1].join(".");
                view_record(
                    "Projection",
                    BTreeMap::from([
                        ("container".into(), code_of(path_expr(&container))),
                        ("field".into(), schema_tag(field)),
                        ("scrutinee".into(), pack(path_expr(&container), None)),
                    ]),
                )
            } else {
                view_record(
                    "Local",
                    BTreeMap::from([
                        ("name".into(), schema_tag(&name)),
                        ("token".into(), schema_tag(&name)),
                    ]),
                )
            }
        }
        ExprKind::Call { function, args } => view_record(
            "Call",
            BTreeMap::from([
                ("callee".into(), code_of(*function.clone())),
                (
                    "args".into(),
                    CValue::Sequence(args.iter().cloned().map(code_of).collect()),
                ),
                (
                    "children".into(),
                    CValue::Sequence(
                        args.iter()
                            .cloned()
                            .map(|arg| pack(arg, None))
                            .collect(),
                    ),
                ),
            ]),
        ),
        ExprKind::FunctionAbs { param, body, .. } | ExprKind::QuoteBind { param, body, .. } => {
            view_record(
                "Closure",
                BTreeMap::from([
                    ("param".into(), schema_tag(param)),
                    ("body".into(), code_of(*body.clone())),
                    ("scope".into(), pack(*body.clone(), Some(param))),
                ]),
            )
        }
        ExprKind::Binary { op, left, right } => {
            let callee = scalar_op_name(*op)
                .map(schema_tag)
                .unwrap_or_else(|| schema_tag(&format!("{op:?}")));
            view_record(
                "Call",
                BTreeMap::from([
                    ("callee".into(), callee),
                    (
                        "args".into(),
                        CValue::Sequence(vec![code_of(*left.clone()), code_of(*right.clone())]),
                    ),
                    (
                        "children".into(),
                        CValue::Sequence(vec![
                            pack(*left.clone(), None),
                            pack(*right.clone(), None),
                        ]),
                    ),
                ]),
            )
        }
        ExprKind::Unary { op, value } => view_record(
            "Call",
            BTreeMap::from([
                ("callee".into(), schema_tag(&format!("{op:?}"))),
                (
                    "args".into(),
                    CValue::Sequence(vec![code_of(*value.clone())]),
                ),
                (
                    "children".into(),
                    CValue::Sequence(vec![pack(*value.clone(), None)]),
                ),
            ]),
        ),
        ExprKind::List(items) | ExprKind::Tuple(items) => view_record(
            "Sequence",
            BTreeMap::from([
                (
                    "elements".into(),
                    CValue::Sequence(items.iter().cloned().map(code_of).collect()),
                ),
                (
                    "children".into(),
                    CValue::Sequence(
                        items
                            .iter()
                            .cloned()
                            .map(|item| pack(item, None))
                            .collect(),
                    ),
                ),
            ]),
        ),
        ExprKind::Quote { body } => view_record(
            "Quotation",
            BTreeMap::from([
                ("body".into(), code_of(*body.clone())),
                ("scope".into(), pack(*body.clone(), None)),
            ]),
        ),
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            let mut fields = BTreeMap::from([
                (
                    "arms".into(),
                    CValue::Sequence(
                        arms.iter()
                            .map(|(cond, value)| {
                                CValue::Tuple(vec![
                                    code_of(cond.clone()),
                                    code_of(value.clone()),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("else".into(), code_of(*else_arm.clone())),
                ("otherwise".into(), code_of(*else_arm.clone())),
                (
                    "children".into(),
                    CValue::Sequence({
                        let mut children = arms
                            .iter()
                            .map(|(_, value)| pack(value.clone(), None))
                            .collect::<Vec<_>>();
                        children.push(pack(*else_arm.clone(), None));
                        children
                    }),
                ),
            ]);
            if let Some(subject) = subject {
                fields.insert("subject".into(), code_of(*subject.clone()));
                fields.insert(
                    "scrutinee".into(),
                    pack(*subject.clone(), None),
                );
            }
            view_record("Cases", fields)
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            let children = CValue::Sequence(vec![
                pack(*condition.clone(), None),
                pack(*then_value.clone(), None),
                pack(*else_value.clone(), None),
            ]);
            view_record(
                "Branch",
                BTreeMap::from([
                    ("condition".into(), code_of(*condition.clone())),
                    ("then".into(), code_of(*then_value.clone())),
                    ("else".into(), code_of(*else_value.clone())),
                    ("otherwise".into(), code_of(*else_value.clone())),
                    ("children".into(), children),
                ]),
            )
        }
        ExprKind::Index { value, indices } => {
            let index_pkgs = indices
                .iter()
                .cloned()
                .map(|index| pack(index, None))
                .collect::<Vec<_>>();
            let mut fields = BTreeMap::from([
                ("container".into(), code_of(*value.clone())),
                (
                    "indices".into(),
                    CValue::Sequence(indices.iter().cloned().map(code_of).collect()),
                ),
                ("children".into(), CValue::Sequence(index_pkgs)),
                ("scrutinee".into(), pack(*value.clone(), None)),
            ]);
            if let Some(index) = indices.first() {
                fields.insert("index".into(), code_of(index.clone()));
            }
            view_record("Index", fields)
        }
        ExprKind::Recur { name, ty, body } => {
            let scope = pack(*body.clone(), Some(name));
            view_record(
                "Recur",
                BTreeMap::from([
                    ("name".into(), schema_tag(name)),
                    ("type".into(), code_of(*ty.clone())),
                    ("body".into(), code_of(*body.clone())),
                    ("scope".into(), scope),
                ]),
            )
        }
        ExprKind::Record { type_path, fields } => {
            let children = fields
                .iter()
                .map(|(name, value)| {
                    CValue::Tuple(vec![schema_tag(name), pack(value.clone(), None)])
                })
                .collect();
            view_record(
                "Record",
                BTreeMap::from([
                    ("schema".into(), schema_tag(&type_path.join("."))),
                    (
                        "fields".into(),
                        CValue::Sequence(
                            fields
                                .iter()
                                .map(|(name, value)| {
                                    CValue::Tuple(vec![schema_tag(name), code_of(value.clone())])
                                })
                                .collect(),
                        ),
                    ),
                    ("children".into(), CValue::Sequence(children)),
                ]),
            )
        }
        ExprKind::SequenceCons { head, tail } => view_record(
            "Sequence",
            BTreeMap::from([
                ("head".into(), code_of(*head.clone())),
                ("tail".into(), code_of(*tail.clone())),
                (
                    "children".into(),
                    CValue::Sequence(vec![pack(*head.clone(), None), pack(*tail.clone(), None)]),
                ),
            ]),
        ),
        _ => view_record("Opaque", BTreeMap::from([("body".into(), code_of(expr.clone()))])),
    }
}

fn scalar_op_name(op: BinaryOp) -> Option<&'static str> {
    match op {
        BinaryOp::Add => Some("Add"),
        BinaryOp::Sub => Some("Sub"),
        BinaryOp::Mul => Some("Mul"),
        BinaryOp::Div => Some("Div"),
        BinaryOp::Eq => Some("Eq"),
        BinaryOp::Ne => Some("Ne"),
        BinaryOp::Lt => Some("Lt"),
        BinaryOp::Le => Some("Le"),
        BinaryOp::Gt => Some("Gt"),
        BinaryOp::Ge => Some("Ge"),
        BinaryOp::And => Some("And"),
        BinaryOp::Or => Some("Or"),
        BinaryOp::Pow => Some("Pow"),
        BinaryOp::Imply => Some("Imply"),
        BinaryOp::Iff => Some("Iff"),
        BinaryOp::Asymp => Some("Asymp"),
        BinaryOp::In => Some("In"),
    }
}

fn function_body_expr(decl: &FnDecl) -> Expr {
    let mut body = if decl.outputs.len() > 1 {
        let items = decl
            .outputs
            .iter()
            .filter_map(|out| {
                decl.defs
                    .iter()
                    .find(|(name, _)| name == out)
                    .map(|(_, expr)| expr.clone())
            })
            .collect::<Vec<_>>();
        dummy_expr(ExprKind::Tuple(items))
    } else {
        decl.output
            .as_ref()
            .and_then(|out| {
                decl.defs
                    .iter()
                    .find(|(name, _)| name == out)
                    .map(|(_, expr)| expr.clone())
            })
            .or_else(|| decl.defs.last().map(|(_, expr)| expr.clone()))
            .unwrap_or_else(|| dummy_expr(ExprKind::Tuple(Vec::new())))
    };
    for param in decl.inputs.iter().rev() {
        body = dummy_expr(ExprKind::FunctionAbs {
            param: param.clone(),
            domain: Box::new(path_expr("Rat")),
            body: Box::new(body),
        });
    }
    body
}

fn record_kind_name(type_name: &str, fields: &BTreeMap<String, CValue>) -> String {
    match fields.get("kind") {
        Some(CValue::Record { type_name, .. }) => type_name.clone(),
        _ => type_name.to_string(),
    }
}

fn as_scalar_op(value: &CValue) -> Option<BinaryOp> {
    let name = match value {
        CValue::Record { type_name, fields } => record_kind_name(type_name, fields),
        CValue::Code(code) => match &code.expr.kind {
            ExprKind::Path { segments, .. } => segments.join("."),
            _ => return None,
        },
        _ => return None,
    };
    match name.as_str() {
        "Add" => Some(BinaryOp::Add),
        "Sub" => Some(BinaryOp::Sub),
        "Mul" => Some(BinaryOp::Mul),
        "Div" => Some(BinaryOp::Div),
        "Eq" => Some(BinaryOp::Eq),
        "Ne" => Some(BinaryOp::Ne),
        "Lt" => Some(BinaryOp::Lt),
        "Le" => Some(BinaryOp::Le),
        "Gt" => Some(BinaryOp::Gt),
        "Ge" => Some(BinaryOp::Ge),
        "And" => Some(BinaryOp::And),
        "Or" => Some(BinaryOp::Or),
        "Pow" => Some(BinaryOp::Pow),
        "Imply" => Some(BinaryOp::Imply),
        "Iff" => Some(BinaryOp::Iff),
        "Asymp" => Some(BinaryOp::Asymp),
        "In" => Some(BinaryOp::In),
        _ => None,
    }
}

fn rebuild_call(fields: &BTreeMap<String, CValue>) -> Result<Expr, String> {
    let args = match fields.get("args") {
        Some(CValue::Sequence(items)) => items
            .iter()
            .map(rebuild_expr)
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("Call missing args".into()),
    };
    let callee = fields.get("callee").ok_or("Call missing callee")?;
    if let Some(op) = as_scalar_op(callee) {
        if args.len() != 2 {
            return Err("scalar call expects two arguments".into());
        }
        return Ok(dummy_expr(ExprKind::Binary {
            op,
            left: Box::new(args[0].clone()),
            right: Box::new(args[1].clone()),
        }));
    }
    Ok(dummy_expr(ExprKind::Call {
        function: Box::new(rebuild_expr(callee)?),
        args,
    }))
}

fn rebuild_expr(value: &CValue) -> Result<Expr, String> {
    match value {
        CValue::Code(code) => Ok(code.expr.clone()),
        CValue::Int(_) | CValue::Rat { .. } | CValue::Bool(_) | CValue::Float64(_) => {
            Ok(value_to_expr(value))
        }
        CValue::Sequence(items) | CValue::Tuple(items) => {
            let exprs = items
                .iter()
                .map(rebuild_expr)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(dummy_expr(ExprKind::Tuple(exprs)))
        }
        CValue::Record { type_name, fields } => {
            let kind = record_kind_name(type_name, fields);
            match kind.as_str() {
                "Literal" => fields
                    .get("value")
                    .map(value_to_expr)
                    .ok_or_else(|| "Literal node missing value".into()),
                "Local" | "Global" => {
                    let name = match fields.get("name") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        Some(CValue::Code(code)) => match &code.expr.kind {
                            ExprKind::Path { segments, .. } => segments.join("."),
                            _ => return Err("reference name is not a path".into()),
                        },
                        _ => type_name.clone(),
                    };
                    Ok(path_expr(&name))
                }
                "Call" => rebuild_call(fields),
                "Closure" => {
                    let param = match fields.get("param") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Closure missing param".into()),
                    };
                    let body = rebuild_expr(fields.get("body").ok_or("Closure missing body")?)?;
                    Ok(dummy_expr(ExprKind::FunctionAbs {
                        param,
                        domain: Box::new(path_expr("Rat")),
                        body: Box::new(body),
                    }))
                }
                "Sequence" => {
                    let elements = match fields.get("elements") {
                        Some(CValue::Sequence(items)) => items,
                        _ => return Err("Sequence missing elements".into()),
                    };
                    let exprs = elements
                        .iter()
                        .map(rebuild_expr)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(dummy_expr(ExprKind::Tuple(exprs)))
                }
                "Fragment" => fields
                    .get("term")
                    .ok_or_else(|| String::from("Fragment missing term"))
                    .and_then(rebuild_expr),
                "Branch" | "If" => {
                    let condition =
                        rebuild_expr(fields.get("condition").ok_or("Branch missing condition")?)?;
                    let then_value =
                        rebuild_expr(fields.get("then").ok_or("Branch missing then")?)?;
                    let else_value = fields
                        .get("otherwise")
                        .or_else(|| fields.get("else"))
                        .ok_or("Branch missing else")?;
                    let else_value = rebuild_expr(else_value)?;
                    Ok(dummy_expr(ExprKind::If {
                        condition: Box::new(condition),
                        then_value: Box::new(then_value),
                        else_value: Box::new(else_value),
                    }))
                }
                "Projection" => {
                    let container =
                        rebuild_expr(fields.get("container").ok_or("Projection missing container")?)?;
                    let field = match fields.get("field") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Projection missing field".into()),
                    };
                    match container.kind {
                        ExprKind::Path { mut segments, generics } => {
                            segments.push(field);
                            Ok(dummy_expr(ExprKind::Path { segments, generics }))
                        }
                        _ => Err("Projection container is not a path".into()),
                    }
                }
                "Index" => {
                    let container =
                        rebuild_expr(fields.get("container").ok_or("Index missing container")?)?;
                    let indices = match fields.get("indices") {
                        Some(CValue::Sequence(items)) => items
                            .iter()
                            .map(rebuild_expr)
                            .collect::<Result<Vec<_>, _>>()?,
                        _ => match fields.get("index") {
                            Some(index) => vec![rebuild_expr(index)?],
                            None => return Err("Index missing index".into()),
                        },
                    };
                    Ok(dummy_expr(ExprKind::Index {
                        value: Box::new(container),
                        indices,
                    }))
                }
                "Quotation" => {
                    let body = rebuild_expr(fields.get("body").ok_or("Quotation missing body")?)?;
                    Ok(dummy_expr(ExprKind::Quote {
                        body: Box::new(body),
                    }))
                }
                "Record" => {
                    let schema = match fields.get("schema") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Record missing schema".into()),
                    };
                    let record_fields = match fields.get("fields") {
                        Some(CValue::Sequence(items)) => items
                            .iter()
                            .map(|item| match item {
                                CValue::Tuple(pair) if pair.len() == 2 => {
                                    let name = match &pair[0] {
                                        CValue::Record { type_name, .. } => type_name.clone(),
                                        _ => return Err("Record field name is not a tag".into()),
                                    };
                                    Ok((name, rebuild_expr(&pair[1])?))
                                }
                                _ => Err(String::from("Record field is not a pair")),
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                        _ => return Err("Record missing fields".into()),
                    };
                    Ok(dummy_expr(ExprKind::Record {
                        type_path: schema.split('.').map(str::to_string).collect(),
                        fields: record_fields,
                    }))
                }
                "Recur" => {
                    let name = match fields.get("name") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Recur missing name".into()),
                    };
                    let ty = rebuild_expr(fields.get("type").ok_or("Recur missing type")?)?;
                    let body = rebuild_expr(fields.get("body").ok_or("Recur missing body")?)?;
                    Ok(dummy_expr(ExprKind::Recur {
                        name,
                        ty: Box::new(ty),
                        body: Box::new(body),
                    }))
                }
                "Match" | "Cases" => {
                    let arms = match fields.get("arms") {
                        Some(CValue::Sequence(items)) => items
                            .iter()
                            .map(|item| match item {
                                CValue::Tuple(pair) if pair.len() == 2 => {
                                    Ok((rebuild_expr(&pair[0])?, rebuild_expr(&pair[1])?))
                                }
                                _ => Err(String::from("Match arm is not a pair")),
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                        _ => return Err("Match missing arms".into()),
                    };
                    let else_arm = fields
                        .get("otherwise")
                        .or_else(|| fields.get("else"))
                        .ok_or("Match missing else")?;
                    let else_arm = rebuild_expr(else_arm)?;
                    let subject = fields
                        .get("subject")
                        .map(rebuild_expr)
                        .transpose()?
                        .map(Box::new);
                    Ok(dummy_expr(ExprKind::Cases {
                        subject,
                        arms,
                        else_arm: Box::new(else_arm),
                    }))
                }
                other => Err(format!("unknown node `{other}`")),
            }
        }
        other => Err(format!("{other}")),
    }
}

fn project_field(value: &CValue, field: &str) -> Option<CValue> {
    match value {
        CValue::Record { fields, .. } => fields.get(field).cloned(),
        CValue::Tuple(items) => tuple_index(field).and_then(|index| items.get(index).cloned()),
        CValue::Sequence(items) if field == "length" => Some(CValue::Int(items.len() as i128)),
        _ => None,
    }
}

fn function_is_opaque(decl: &Declaration) -> bool {
    for section in decl.sections().filter(|section| section.name == "exports") {
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::FieldDecl { ty, .. } => {
                    if format!("{ty:?}").contains("opaque") {
                        return true;
                    }
                }
                StmtKind::Assign { value, .. } => {
                    if let ExprKind::Path { segments, .. } = &value.kind {
                        if segments.iter().any(|segment| segment == "opaque") {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    false
}

fn object_representation_kind(decl: &Declaration) -> String {
    if let Some(kind) = section_text(decl, "representation", "kind")
        .or_else(|| section_text(decl, "representation", "shape"))
    {
        return kind;
    }
    decl.sections()
        .find(|section| section.name == "representation")
        .and_then(|section| {
            section.suite.statements.first().and_then(|stmt| match &stmt.kind {
                StmtKind::FieldDecl { name, ty, .. } => {
                    if name == "abstract" || name == "package" {
                        Some(name.clone())
                    } else if format!("{ty:?}").contains("abstract") {
                        Some("abstract".into())
                    } else if format!("{ty:?}").contains("package") {
                        Some("package".into())
                    } else {
                        None
                    }
                }
                StmtKind::Assign { value, .. } => match &value.kind {
                    ExprKind::Path { segments, .. } => segments.last().cloned(),
                    ExprKind::Call { function, .. } => match &function.kind {
                        ExprKind::Path { segments, .. } => segments.last().cloned(),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
        })
        .unwrap_or_else(|| "record".into())
}

fn section_assigns(decl: &Declaration, name: &str) -> Vec<(String, Expr)> {
    decl.sections()
        .filter(|section| section.name == name)
        .flat_map(|section| {
            section.suite.statements.iter().filter_map(|stmt| {
                if let StmtKind::Assign { target, value } = &stmt.kind {
                    target.segments.first().cloned().map(|name| (name, value.clone()))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn section_fields(decl: &Declaration, name: &str) -> Vec<String> {
    section_typed_fields(decl, name)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

fn section_typed_fields(decl: &Declaration, name: &str) -> Vec<(String, TypeExpr)> {
    decl.sections()
        .filter(|section| section.name == name)
        .flat_map(|section| {
            section.suite.statements.iter().filter_map(|stmt| {
                if let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                    Some((name.clone(), ty.clone()))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn admit_input_type(ty: &TypeExpr, value: &CValue) -> Result<(), ConstructorError> {
    if type_admits(ty, value) {
        Ok(())
    } else {
        Err(fault(
            "type",
            format!("value `{value}` does not have the declared input type"),
        ))
    }
}

fn type_admits(ty: &TypeExpr, value: &CValue) -> bool {
    match &ty.kind {
        TypeKind::Fn { .. } => matches!(value, CValue::Closure(_)),
        TypeKind::List(_) => matches!(value, CValue::Sequence(_)),
        TypeKind::Tuple(_) => matches!(value, CValue::Tuple(_) | CValue::Record { .. }),
        TypeKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int") => matches!(value, CValue::Int(_)),
            Some("Bool") => matches!(value, CValue::Bool(_)),
            Some("Rat") => matches!(value, CValue::Rat { .. } | CValue::Int(_)),
            Some("Float64") => matches!(value, CValue::Float64(_)),
            Some("Code") => matches!(value, CValue::Code(_)),
            Some("sequence") => matches!(value, CValue::Sequence(_)),
            _ => true,
        },
        _ => true,
    }
}

fn section_text(decl: &Declaration, section: &str, field: &str) -> Option<String> {
    for s in decl.sections().filter(|s| s.name == section) {
        for stmt in &s.suite.statements {
            if let StmtKind::Assign { target, value } = &stmt.kind {
                if target.segments.first().map(String::as_str) == Some(field) {
                    if let ExprKind::Path { segments, .. } = &value.kind {
                        return Some(segments.join("."));
                    }
                    if let ExprKind::Str(text) = &value.kind {
                        return Some(text.clone());
                    }
                }
            }
            if let StmtKind::FieldDecl { name, default, .. } = &stmt.kind {
                if name == field {
                    if let Some(ExprKind::Path { segments, .. }) = default.as_ref().map(|e| &e.kind)
                    {
                        return Some(segments.join("."));
                    }
                }
            }
        }
    }
    None
}

/// Type-admit a constructor module with the same environment as evaluation.
const CONSTRUCTOR_SECTIONS: &[&str] = &[
    "parameters",
    "representation",
    "invariants",
    "inputs",
    "outputs",
    "definitions",
    "question",
    "using",
    "answer",
    "budget",
    "tests",
    "exports",
];

fn admit_constructor_surface(tree: &SyntaxTree) -> Result<(), ConstructorError> {
    for item in &tree.items {
        match item {
            Item::Package { .. } | Item::Use { .. } => {}
            Item::Notation(_) => {
                return Err(fault(
                    "E-KIND-GONE",
                    "notation aliases are not constructor surface; write the scalar operator or `use` an ordinary module",
                ));
            }
            Item::Declaration(decl) => {
                if !matches!(decl.as_kind.as_str(), "object" | "function" | "query") {
                    return Err(fault(
                        "E-KIND-GONE",
                        format!(
                            "declaration kind `{}` is not a core kind; write `emath object`, `emath function`, or `emath query`",
                            decl.as_kind
                        ),
                    ));
                }
                for section in decl.sections() {
                    if !CONSTRUCTOR_SECTIONS.contains(&section.name.as_str()) {
                        return Err(fault(
                            "E-SEC-101",
                            format!(
                                "`{}:` is not a constructor section",
                                section.name
                            ),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn admit_tree(tree: &SyntaxTree) -> Result<(), ConstructorError> {
    admit_tree_at(tree, None)
}

/// Type-admit a module, resolving `use` paths from `source` or the repo roots.
pub fn admit_tree_at(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<(), ConstructorError> {
    let mut engine = empty_engine();
    install_local_items(&mut engine, tree)?;
    load_imports(
        &mut engine,
        tree,
        &module_roots_for(source),
        &mut BTreeSet::new(),
    )?;
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        match decl.as_kind.as_str() {
            "function" => admit_function(&engine, decl)?,
            "query" => admit_query(&engine, decl)?,
            "object" => {}
            other => {
                return Err(fault(
                    "E-KIND-GONE",
                    format!("declaration kind `{other}` is not a core kind"),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
enum CType {
    Bool,
    Int,
    Rat,
    Float64,
    Sequence,
    Tuple,
    Record,
    Closure,
    Code,
    Receipt,
    Schema,
    Unknown,
}

impl CType {
    fn join(&self, other: &Self) -> Option<Self> {
        if self == other {
            return Some(self.clone());
        }
        match (self, other) {
            (Self::Unknown, t) | (t, Self::Unknown) => Some(t.clone()),
            (Self::Int, Self::Rat) | (Self::Rat, Self::Int) => Some(Self::Rat),
            (Self::Int | Self::Rat, Self::Float64) | (Self::Float64, Self::Int | Self::Rat) => {
                Some(Self::Float64)
            }
            (Self::Schema, Self::Record) | (Self::Record, Self::Schema) => Some(Self::Record),
            _ => None,
        }
    }

    fn conforms(&self, declared: &Self) -> bool {
        self == declared
            || matches!(
                (self, declared),
                (Self::Int, Self::Rat)
                    | (Self::Unknown, _)
                    | (_, Self::Unknown)
                    | (Self::Schema, Self::Record)
                    | (Self::Record, Self::Schema)
            )
    }
}

fn ctype_from_type(ty: &TypeExpr) -> CType {
    match &ty.kind {
        TypeKind::Fn { .. } => CType::Closure,
        TypeKind::List(_) => CType::Sequence,
        TypeKind::Tuple(_) => CType::Tuple,
        TypeKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int") | Some("Nat") => CType::Int,
            Some("Bool") => CType::Bool,
            Some("Rat") => CType::Rat,
            Some("Float64") | Some("F64") => CType::Float64,
            Some("Code") => CType::Code,
            Some("sequence") | Some("Sequence") => CType::Sequence,
            _ => CType::Unknown,
        },
        _ => CType::Unknown,
    }
}

fn admit_function(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
    let mut types = BTreeMap::new();
    for (name, ty) in section_typed_fields(decl, "inputs") {
        types.insert(name, ctype_from_type(&ty));
    }
    let outputs: BTreeMap<String, CType> = section_typed_fields(decl, "outputs")
        .into_iter()
        .map(|(name, ty)| (name, ctype_from_type(&ty)))
        .collect();
    for (name, expr) in section_assigns(decl, "definitions") {
        let got = engine.infer(&types, &expr)?;
        if let Some(declared) = outputs.get(&name)
            && !got.conforms(declared)
        {
            return Err(fault(
                "type",
                format!("definition `{name}` does not have the declared output type"),
            ));
        }
        types.insert(name, got);
    }
    for name in outputs.keys() {
        if !types.contains_key(name) {
            return Err(fault(
                "type",
                format!("output `{name}` has no definition"),
            ));
        }
    }
    admit_tests(engine, decl, &types)
}

fn admit_query(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
    let mut types = BTreeMap::new();
    for (name, ty) in section_typed_fields(decl, "inputs") {
        types.insert(name, ctype_from_type(&ty));
    }
    for (name, expr) in section_assigns(decl, "definitions") {
        let got = engine.infer(&types, &expr)?;
        types.insert(name, got);
    }
    types.insert("receipt".into(), CType::Receipt);
    types.insert("diagnostic".into(), CType::Record);
    admit_tests(engine, decl, &types)
}

fn admit_tests(
    engine: &Engine,
    decl: &Declaration,
    types: &BTreeMap<String, CType>,
) -> Result<(), ConstructorError> {
    for section in decl.sections().filter(|section| section.name == "tests") {
        for stmt in &section.suite.statements {
            let StmtKind::Section(example) = &stmt.kind else {
                continue;
            };
            if example.name != "example" {
                continue;
            }
            let mut local = types.clone();
            for inner in &example.suite.statements {
                match &inner.kind {
                    StmtKind::Given { name, value } => {
                        if !local.contains_key(name) {
                            return Err(fault(
                                "unbound",
                                format!("`given` name `{name}` is not an input"),
                            ));
                        }
                        let _ = engine.infer(&local, value)?;
                    }
                    StmtKind::Expect(expr) => {
                        let got = engine.infer(&local, expr)?;
                        if !got.conforms(&CType::Bool) {
                            return Err(fault("type", "`expect` must be Bool"));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

impl Engine {
    fn infer(&self, types: &BTreeMap<String, CType>, expr: &Expr) -> Result<CType, ConstructorError> {
        match &expr.kind {
            ExprKind::Int(_) => Ok(CType::Int),
            ExprKind::Rational { .. } => Ok(CType::Rat),
            ExprKind::Float(_) => Ok(CType::Float64),
            ExprKind::Bool(_) => Ok(CType::Bool),
            ExprKind::Path { segments, .. } => self.infer_path(types, segments),
            ExprKind::Unary { value, .. } => self.infer(types, value),
            ExprKind::Binary { op, left, right } => {
                let l = self.infer(types, left)?;
                let r = self.infer(types, right)?;
                Ok(match op {
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt
                    | BinaryOp::Ge | BinaryOp::And | BinaryOp::Or | BinaryOp::Imply
                    | BinaryOp::Iff => CType::Bool,
                    BinaryOp::Div => match (l, r) {
                        (CType::Float64, _) | (_, CType::Float64) => CType::Float64,
                        (CType::Unknown, _) | (_, CType::Unknown) => CType::Unknown,
                        _ => CType::Rat,
                    },
                    _ => l.join(&r).unwrap_or(CType::Unknown),
                })
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let cond = self.infer(types, condition)?;
                if !cond.conforms(&CType::Bool) {
                    return Err(fault("type", "if condition must be Bool"));
                }
                let then_ty = self.infer(types, then_value)?;
                let else_ty = self.infer(types, else_value)?;
                then_ty
                    .join(&else_ty)
                    .ok_or_else(|| fault("type", "if branches must have the same type"))
            }
            ExprKind::List(_) | ExprKind::SequenceCons { .. } | ExprKind::Range { .. } => {
                Ok(CType::Sequence)
            }
            ExprKind::Tuple(_) => Ok(CType::Tuple),
            ExprKind::Record { .. } => Ok(CType::Record),
            ExprKind::Index { value, indices } => {
                let _ = self.infer(types, value)?;
                if let Some(index) = indices.first() {
                    let _ = self.infer(types, index)?;
                }
                Ok(CType::Unknown)
            }
            ExprKind::Call { function, args } => self.infer_call(types, function, args),
            ExprKind::FunctionAbs { param, domain, body } => {
                let mut inner = types.clone();
                inner.insert(param.clone(), ctype_from_type_expr_or_path(domain));
                let _ = self.infer(&inner, body)?;
                Ok(CType::Closure)
            }
            ExprKind::Recur { ty, body, .. } => {
                if !is_fn_type(ty) {
                    return Err(fault(
                        "type",
                        "recur binder type must be a function type A -> B or a record of function types",
                    ));
                }
                if !is_guarded_recur_body(body) {
                    return Err(fault(
                        "unguarded_recursive_binding",
                        "recur body must be a function literal or a record of function literals",
                    ));
                }
                Ok(CType::Closure)
            }
            ExprKind::Quote { .. } | ExprKind::QuoteBind { .. } => Ok(CType::Code),
            ExprKind::CallableBinder { callee, domain, .. } => {
                let _ = self.infer(types, callee)?;
                let _ = self.infer(types, domain)?;
                Ok(CType::Unknown)
            }
            ExprKind::Cases {
                subject,
                arms,
                else_arm,
            } => {
                if let Some(subject) = subject {
                    let _ = self.infer(types, subject)?;
                }
                let mut result = self.infer(types, else_arm)?;
                for (cond, value) in arms {
                    let _ = self.infer(types, cond)?;
                    let arm = self.infer(types, value)?;
                    result = result
                        .join(&arm)
                        .ok_or_else(|| fault("type", "cases arms must have the same type"))?;
                }
                Ok(result)
            }
            _ => Ok(CType::Unknown),
        }
    }

    fn infer_path(
        &self,
        types: &BTreeMap<String, CType>,
        segments: &[String],
    ) -> Result<CType, ConstructorError> {
        let name = segments.join(".");
        if let Some(ty) = types.get(&name) {
            return Ok(ty.clone());
        }
        if segments.len() == 2 {
            if let Some(ty) = types.get(&segments[0]) {
                return Ok(match (ty, segments[1].as_str()) {
                    (CType::Sequence, "length") => CType::Int,
                    // Projection type is not reconstructed from a record tag.
                    // Unknown conforms to a declared field type; Schema does not.
                    (CType::Receipt, _)
                    | (CType::Record, _)
                    | (CType::Tuple, _)
                    | (CType::Code, _)
                    | (CType::Schema, _)
                    | (CType::Unknown, _) => CType::Unknown,
                    _ => CType::Unknown,
                });
            }
            if self.objects.contains_key(&segments[0]) {
                return Ok(match segments[1].as_str() {
                    "pack" | "open" => CType::Closure,
                    _ => CType::Unknown,
                });
            }
        }
        if matches!(name.as_str(), "true" | "false") {
            return Ok(CType::Bool);
        }
        if is_schema_tag(&name) {
            return Ok(CType::Schema);
        }
        if self.functions.contains_key(&name) {
            return Ok(CType::Closure);
        }
        if self.objects.contains_key(&name) {
            return Ok(CType::Record);
        }
        if self.queries.contains_key(&name) {
            return Ok(CType::Unknown);
        }
        Err(fault("unbound", format!("unbound `{name}`")))
    }

    fn infer_call(
        &self,
        types: &BTreeMap<String, CType>,
        function: &Expr,
        args: &[Expr],
    ) -> Result<CType, ConstructorError> {
        if let ExprKind::Path { segments, .. } = &function.kind {
            let name = segments.join(".");
            if name == "transformation_rule_unavailable" || name.ends_with(".opaque") {
                return Ok(CType::Unknown);
            }
            if name == "length" {
                for arg in args {
                    let _ = self.infer(types, arg)?;
                }
                return Ok(CType::Int);
            }
            if name.starts_with("quote.") {
                for arg in args {
                    let _ = self.infer_quote_arg(types, arg)?;
                }
                return Ok(CType::Unknown);
            }
            if segments.len() == 2 && segments[1] == "pack" {
                for arg in args {
                    let _ = self.infer(types, arg)?;
                }
                return Ok(CType::Record);
            }
            if let Some(decl) = self.functions.get(&name) {
                for arg in args {
                    let _ = self.infer(types, arg)?;
                }
                return Ok(match decl.output_types.as_slice() {
                    [ty] => ctype_from_type(ty),
                    [] => CType::Unknown,
                    _ => CType::Record,
                });
            }
            if is_refused_recipe(&name) {
                return Err(fault(
                    "method_unavailable",
                    format!("`{name}` is an ordinary module method, not a constructor operation"),
                ));
            }
            // Postfix `.field` on a non-path is parsed as `field(recv)`.
            if segments.len() == 1 && args.len() == 1 && !self.is_typed_callee(types, &name) {
                let recv = self.infer(types, &args[0])?;
                if matches!(
                    recv,
                    CType::Record
                        | CType::Tuple
                        | CType::Receipt
                        | CType::Code
                        | CType::Schema
                        | CType::Unknown
                ) {
                    return Ok(CType::Unknown);
                }
            }
        }
        let _ = self.infer(types, function)?;
        for arg in args {
            let _ = self.infer(types, arg)?;
        }
        Ok(CType::Unknown)
    }

    fn is_typed_callee(&self, types: &BTreeMap<String, CType>, name: &str) -> bool {
        self.functions.contains_key(name)
            || matches!(types.get(name), Some(CType::Closure))
    }

    fn infer_quote_arg(
        &self,
        types: &BTreeMap<String, CType>,
        expr: &Expr,
    ) -> Result<CType, ConstructorError> {
        match self.infer(types, expr) {
            Ok(ty) => Ok(ty),
            Err(err) if err.code == "unbound" => match &expr.kind {
                ExprKind::Path { segments, .. } if segments.len() == 1 => Ok(CType::Schema),
                _ => Err(err),
            },
            Err(err) => Err(err),
        }
    }
}

fn ctype_from_type_expr_or_path(expr: &Expr) -> CType {
    match &expr.kind {
        ExprKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int") | Some("Nat") => CType::Int,
            Some("Bool") => CType::Bool,
            Some("Rat") => CType::Rat,
            Some("Float64") | Some("F64") => CType::Float64,
            Some("Code") => CType::Code,
            _ => CType::Unknown,
        },
        ExprKind::Call { function, .. } if is_fn_ctor(function) => CType::Closure,
        _ => CType::Unknown,
    }
}

/// Evaluate a parsed constructor-layer module.
pub fn evaluate_tree(tree: &SyntaxTree) -> Result<ModuleReport, ConstructorError> {
    evaluate_tree_at(tree, None)
}

/// Evaluate a module, resolving `use` paths from `source` or the repo roots.
pub fn evaluate_tree_at(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<ModuleReport, ConstructorError> {
    let mut engine = empty_engine();
    let mut uses = Vec::new();
    install_local_items(&mut engine, tree)?;
    load_imports(&mut engine, tree, &module_roots_for(source), &mut BTreeSet::new())?;
    for item in &tree.items {
        if let Item::Use { path, .. } = item {
            uses.push(path.join("."));
        }
    }
    for item in &tree.items {
        match item {
            Item::Use { .. } => {}
            Item::Declaration(decl) => match decl.as_kind.as_str() {
                "function" | "query" | "object" => {}
                other => {
                    return Err(fault(
                        "E-KIND-GONE",
                        format!("declaration kind `{other}` is not a core kind"),
                    ));
                }
            },
            _ => {}
        }
    }

    let mut tests = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        for section in decl.sections().filter(|section| section.name == "tests") {
            let mut label = decl.name.clone();
            let mut givens = BTreeMap::new();
            let mut expects = Vec::new();
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Section(example) if example.name == "example" => {
                        if let Some(generic) = &example.generic {
                            label = generic.clone();
                        }
                        for inner in &example.suite.statements {
                            match &inner.kind {
                                StmtKind::Given { name, value } => {
                                    givens.insert(name.clone(), engine.eval(value)?);
                                }
                                StmtKind::Expect(expr) => expects.push(expr.clone()),
                                StmtKind::Assign { target, value }
                                    if target.segments.first().map(String::as_str) == Some("given")
                                    => {}
                                _ => {}
                            }
                        }
                    }
                    StmtKind::Given { name, value } => {
                        givens.insert(name.clone(), engine.eval(value)?);
                    }
                    StmtKind::Expect(expr) => expects.push(expr.clone()),
                    _ => {}
                }
            }
            if decl.as_kind == "query" {
                let receipt = engine.run_query(&decl.name, &givens)?;
                for (name, value) in &givens {
                    engine.env.insert(name.clone(), value.clone());
                }
                if let Some(qdecl) = engine.queries.get(&decl.name).cloned() {
                    for (name, expr) in &qdecl.defs {
                        if let Ok(value) = engine.eval(expr) {
                            engine.env.insert(name.clone(), value);
                        }
                    }
                }
                engine.env.insert(
                    "receipt".into(),
                    CValue::Receipt(Box::new(receipt.clone())),
                );
                engine.env.insert(
                    "diagnostic".into(),
                    CValue::Record {
                        type_name: "Diagnostic".into(),
                        fields: BTreeMap::from([(
                            "code".into(),
                            CValue::Record {
                                type_name: receipt
                                    .diagnostic_code
                                    .clone()
                                    .unwrap_or_default(),
                                fields: BTreeMap::new(),
                            },
                        )]),
                    },
                );
                let mut passed = true;
                let mut detail = receipt.fulfillment.clone();
                for expect in expects {
                    match engine.eval(&expect) {
                        Ok(CValue::Bool(true)) => {}
                        Ok(other) => {
                            passed = false;
                            detail = format!("expect produced {other}");
                        }
                        Err(err) => {
                            passed = false;
                            detail = err.message;
                        }
                    }
                }
                tests.push(TestObservation {
                    label,
                    passed,
                    detail,
                    receipt: Some(receipt),
                });
            } else if let Some(fndecl) = engine.functions.get(&decl.name).cloned() {
                let saved = engine.env.clone();
                for (name, value) in &givens {
                    engine.env.insert(name.clone(), value.clone());
                }
                let mut ok = true;
                let mut detail = String::new();
                match engine.eval_fn(
                    &decl.name,
                    &fndecl,
                    &fndecl
                        .inputs
                        .iter()
                        .map(|name| Expr {
                            kind: ExprKind::Path {
                                segments: vec![name.clone()],
                                generics: None,
                            },
                            source: decl.source,
                        })
                        .collect::<Vec<_>>(),
                ) {
                    Ok(value) => {
                        if let Some(output) = &fndecl.output {
                            engine.env.insert(output.clone(), value.clone());
                        }
                        engine.env.insert("result".into(), value);
                        for (name, expr) in &fndecl.defs {
                            if let Ok(def) = engine.eval(expr) {
                                engine.env.insert(name.clone(), def);
                            }
                        }
                    }
                    Err(err) => {
                        ok = false;
                        detail = err.message;
                    }
                }
                for expect in expects {
                    if !ok {
                        break;
                    }
                    match engine.eval(&expect) {
                        Ok(CValue::Bool(true)) => {}
                        Ok(other) => {
                            ok = false;
                            detail = format!("expect produced {other}");
                        }
                        Err(err) => {
                            ok = false;
                            detail = err.message;
                        }
                    }
                }
                engine.env = saved;
                tests.push(TestObservation {
                    label,
                    passed: ok,
                    detail,
                    receipt: None,
                });
            }
        }
    }

    let mut bindings = BTreeMap::new();
    for name in engine.functions.keys() {
        bindings.insert(name.clone(), CValue::Unit);
    }
    Ok(ModuleReport {
        bindings,
        tests,
        uses,
    })
}

/// Evaluate a named function from a parsed module.
pub fn evaluate_function(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
) -> Result<CValue, ConstructorError> {
    let mut engine = engine_from_tree(tree)?;
    let decl = engine
        .functions
        .get(name)
        .cloned()
        .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
    let args: Vec<Expr> = decl
        .inputs
        .iter()
        .map(|input| {
            let value = inputs.get(input).cloned().unwrap_or(CValue::Absent);
            engine.env.insert(input.clone(), value);
            Expr {
                kind: ExprKind::Path {
                    segments: vec![input.clone()],
                    generics: None,
                },
                source: emath_core::Span::default(),
            }
        })
        .collect();
    engine.eval_fn(name, &decl, &args)
}

enum Folded {
    Value(CValue),
    Residual(Expr),
}

/// Quote-eliminate a function to residual expressions over its runtime inputs.
///
/// Compile-time `quote` operations run on the constructor VM. `quote.evaluate`
/// of transformed code becomes the residual body that emission can lower.
/// Opaque or missing transformation rules stay unresolved.
pub fn residual_output_exprs(
    tree: &SyntaxTree,
    name: &str,
) -> Result<Vec<(String, Expr)>, ConstructorError> {
    let mut engine = engine_from_tree(tree)?;
    let decl = engine
        .functions
        .get(name)
        .cloned()
        .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
    let runtime: BTreeSet<String> = decl.inputs.iter().cloned().collect();
    let mut residuals = BTreeMap::new();
    for (def_name, expr) in &decl.defs {
        match fold_expr(&mut engine, expr, &runtime, &residuals)? {
            Folded::Value(value) => {
                engine.env.insert(def_name.clone(), value);
            }
            Folded::Residual(residual) => {
                residuals.insert(def_name.clone(), residual);
            }
        }
    }
    let wanted = if decl.outputs.is_empty() {
        decl.defs
            .last()
            .map(|(def_name, _)| vec![def_name.clone()])
            .unwrap_or_default()
    } else {
        decl.outputs.clone()
    };
    let mut outputs = Vec::new();
    for output in wanted {
        if let Some(residual) = residuals.get(&output) {
            outputs.push((output, residual.clone()));
            continue;
        }
        let value = engine.env.get(&output).ok_or_else(|| {
            fault("unbound", format!("missing output `{output}`"))
        })?;
        if !cvalue_emittable(value) {
            return Err(fault(
                "unresolved",
                format!("output `{output}` is not a scalar residual"),
            ));
        }
        outputs.push((output, value_to_expr(value)));
    }
    Ok(outputs)
}

fn cvalue_emittable(value: &CValue) -> bool {
    match value {
        CValue::Bool(_) | CValue::Int(_) | CValue::Rat { .. } | CValue::Float64(_) => true,
        CValue::Sequence(items) | CValue::Tuple(items) => items.iter().all(cvalue_emittable),
        CValue::Record { fields, .. } => fields.values().all(cvalue_emittable),
        _ => false,
    }
}

fn fold_expr(
    engine: &mut Engine,
    expr: &Expr,
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
) -> Result<Folded, ConstructorError> {
    if let Some(name) = call_path(expr) {
        if name == "quote.evaluate" {
            let arg = call_arg(expr, 0)?;
            return match fold_expr(engine, arg, runtime, residuals)? {
                Folded::Value(CValue::Code(code)) => Ok(Folded::Residual(code.expr)),
                Folded::Value(other) => Ok(Folded::Value(other)),
                Folded::Residual(inner) => Ok(Folded::Residual(inner)),
            };
        }
        if name == "quote.substitute" {
            let fragment = fold_expr(engine, call_arg(expr, 0)?, runtime, residuals)?;
            let reference = substitute_reference(engine, call_arg(expr, 1)?, runtime, residuals)?;
            let replacement = match fold_expr(engine, call_arg(expr, 2)?, runtime, residuals)? {
                Folded::Value(value) => value_to_expr(&value),
                Folded::Residual(inner) => inner,
            };
            let term = match fragment {
                Folded::Value(value) => {
                    fragment_term(value).map_err(|message| fault("type", message))?
                }
                Folded::Residual(inner) => inner,
            };
            return Ok(Folded::Value(CValue::Code(Box::new(Code {
                expr: substitute_path(&term, &reference, &replacement),
            }))));
        }
    }
    if let ExprKind::Path { segments, .. } = &expr.kind {
        if segments.len() == 1 {
            if let Some(residual) = residuals.get(&segments[0]) {
                return Ok(Folded::Residual(residual.clone()));
            }
            if runtime.contains(&segments[0]) && !engine.env.contains_key(&segments[0]) {
                return Ok(Folded::Residual(expr.clone()));
            }
        }
        if segments.len() == 2 {
            if let Some(residual) = residuals.get(&segments[0]) {
                return project_residual(residual, &segments[1]);
            }
        }
    }
    match engine.eval(expr) {
        Ok(value) => Ok(Folded::Value(value)),
        Err(err) if err.code == "unbound" => {
            fold_runtime_expr(engine, expr, runtime, residuals, err)
        }
        Err(err) => Err(err),
    }
}

fn fold_runtime_expr(
    engine: &mut Engine,
    expr: &Expr,
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
    unbound: ConstructorError,
) -> Result<Folded, ConstructorError> {
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            let left = folded_to_expr(fold_expr(engine, left, runtime, residuals)?);
            let right = folded_to_expr(fold_expr(engine, right, runtime, residuals)?);
            Ok(Folded::Residual(dummy_expr(ExprKind::Binary {
                op: *op,
                left: Box::new(left),
                right: Box::new(right),
            })))
        }
        ExprKind::Unary { op, value } => {
            let value = folded_to_expr(fold_expr(engine, value, runtime, residuals)?);
            Ok(Folded::Residual(dummy_expr(ExprKind::Unary {
                op: *op,
                value: Box::new(value),
            })))
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => Ok(Folded::Residual(dummy_expr(ExprKind::If {
            condition: Box::new(folded_to_expr(fold_expr(
                engine, condition, runtime, residuals,
            )?)),
            then_value: Box::new(folded_to_expr(fold_expr(
                engine, then_value, runtime, residuals,
            )?)),
            else_value: Box::new(folded_to_expr(fold_expr(
                engine, else_value, runtime, residuals,
            )?)),
        }))),
        ExprKind::List(items) => Ok(Folded::Residual(dummy_expr(ExprKind::List(fold_expr_list(
            engine, items, runtime, residuals,
        )?)))),
        ExprKind::Tuple(items) => Ok(Folded::Residual(dummy_expr(ExprKind::Tuple(fold_expr_list(
            engine, items, runtime, residuals,
        )?)))),
        ExprKind::Index { value, indices } => Ok(Folded::Residual(dummy_expr(ExprKind::Index {
            value: Box::new(folded_to_expr(fold_expr(engine, value, runtime, residuals)?)),
            indices: fold_expr_list(engine, indices, runtime, residuals)?,
        }))),
        _ => Err(unbound),
    }
}

fn fold_expr_list(
    engine: &mut Engine,
    items: &[Expr],
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
) -> Result<Vec<Expr>, ConstructorError> {
    items
        .iter()
        .map(|item| fold_expr(engine, item, runtime, residuals).map(folded_to_expr))
        .collect()
}

fn folded_to_expr(folded: Folded) -> Expr {
    match folded {
        Folded::Value(value) => value_to_expr(&value),
        Folded::Residual(expr) => expr,
    }
}

fn project_residual(expr: &Expr, field: &str) -> Result<Folded, ConstructorError> {
    if let Some(index) = tuple_index(field) {
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => items
                .get(index)
                .cloned()
                .map(Folded::Residual)
                .ok_or_else(|| fault("invalid_index", format!("no element `{field}`"))),
            _ => Ok(Folded::Residual(dummy_expr(ExprKind::Index {
                value: Box::new(expr.clone()),
                indices: vec![dummy_expr(ExprKind::Int(index.to_string()))],
            }))),
        }
    } else {
        match &expr.kind {
            ExprKind::Record { fields, .. } => fields
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, value)| Folded::Residual(value.clone()))
                .ok_or_else(|| fault("unbound", format!("no field `{field}`"))),
            ExprKind::Path { segments, generics } => {
                let mut segments = segments.clone();
                segments.push(field.to_string());
                Ok(Folded::Residual(dummy_expr(ExprKind::Path {
                    segments,
                    generics: generics.clone(),
                })))
            }
            _ => Err(fault(
                "unresolved",
                format!("cannot project `{field}` from residual"),
            )),
        }
    }
}

fn call_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Call { function, .. } => match &function.kind {
            ExprKind::Path { segments, .. } => Some(segments.join(".")),
            _ => None,
        },
        _ => None,
    }
}

fn call_arg(expr: &Expr, index: usize) -> Result<&Expr, ConstructorError> {
    match &expr.kind {
        ExprKind::Call { args, .. } => args.get(index).ok_or_else(|| {
            fault("arity", format!("call is missing argument {index}"))
        }),
        _ => Err(fault("type", "expected a call")),
    }
}

fn substitute_reference(
    engine: &mut Engine,
    expr: &Expr,
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
) -> Result<String, ConstructorError> {
    match &expr.kind {
        ExprKind::Str(name) => Ok(name.clone()),
        ExprKind::Path { segments, .. } if segments.len() == 1 => Ok(segments[0].clone()),
        _ => match fold_expr(engine, expr, runtime, residuals)? {
            Folded::Value(CValue::Record { type_name, .. }) => Ok(type_name),
            Folded::Value(CValue::Code(code)) => match &code.expr.kind {
                ExprKind::Path { segments, .. } => Ok(segments.join(".")),
                _ => Err(fault(
                    "type",
                    "quote.substitute reference must name a binder",
                )),
            },
            Folded::Residual(inner) => match &inner.kind {
                ExprKind::Path { segments, .. } => Ok(segments.join(".")),
                _ => Err(fault(
                    "type",
                    "quote.substitute reference must name a binder",
                )),
            },
            Folded::Value(other) => Err(fault(
                "type",
                format!("quote.substitute reference must name a binder, found {other}"),
            )),
        },
    }
}

/// Evaluate a named query.
pub fn evaluate_query(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
) -> Result<Receipt, ConstructorError> {
    let mut engine = engine_from_tree(tree)?;
    engine.run_query(name, inputs)
}

/// Evaluate a function under a work budget, returning a checkpoint on suspension.
pub fn evaluate_function_budgeted(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    work_limit: u64,
    checkpoint: Option<&Checkpoint>,
    source_id: &str,
) -> Result<CValue, (ConstructorError, Checkpoint)> {
    evaluate_function_budgeted_at(tree, name, inputs, work_limit, checkpoint, source_id, None, "")
}

/// Budgeted evaluation with an optional source path (for `use`) and source text (for CLI resume).
pub fn evaluate_function_budgeted_at(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    work_limit: u64,
    checkpoint: Option<&Checkpoint>,
    source_id: &str,
    source_path: Option<&Path>,
    source_text: &str,
) -> Result<CValue, (ConstructorError, Checkpoint)> {
    let mut engine = engine_from_tree_at(tree, source_path).map_err(|err| {
        (
            err,
            Checkpoint {
                schema: CHECKPOINT_SCHEMA.into(),
                source_id: source_id.into(),
                image: IMAGE_IDENTITY.into(),
                abi: CHECKPOINT_ABI.into(),
                work: 0,
                remaining: work_limit,
                accounting: ACCOUNTING_VERSION.into(),
                memo: BTreeMap::new(),
                frames: Vec::new(),
                scopes: BTreeSet::new(),
                function: name.into(),
                source: source_text.into(),
                inputs: inputs.clone(),
                next_ref: 0,
            },
        )
    })?;
    engine.work_limit = work_limit;
    engine.source_id = source_id.into();
    engine.entry = name.into();
    engine.source_text = source_text.into();
    engine.inputs = inputs.clone();
    if let Some(checkpoint) = checkpoint {
        engine.restore(checkpoint).map_err(|err| (err, checkpoint.clone()))?;
    }
    match evaluate_function_on(&mut engine, name, inputs) {
        Ok(value) => Ok(value),
        Err(err) => Err((err, engine.snapshot())),
    }
}

fn evaluate_function_on(
    engine: &mut Engine,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
) -> Result<CValue, ConstructorError> {
    if !engine.resume_frames.is_empty() {
        return engine.finish_from_stack();
    }
    let decl = engine
        .functions
        .get(name)
        .cloned()
        .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
    let args: Vec<Expr> = decl
        .inputs
        .iter()
        .map(|input| {
            let value = inputs.get(input).cloned().unwrap_or(CValue::Absent);
            engine.env.insert(input.clone(), value);
            Expr {
                kind: ExprKind::Path {
                    segments: vec![input.clone()],
                    generics: None,
                },
                source: emath_core::Span::default(),
            }
        })
        .collect();
    engine.eval_fn(name, &decl, &args)
}

fn engine_from_tree(tree: &SyntaxTree) -> Result<Engine, ConstructorError> {
    engine_from_tree_at(tree, None)
}

fn engine_from_tree_at(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<Engine, ConstructorError> {
    let mut engine = empty_engine();
    install_local_items(&mut engine, tree)?;
    load_imports(&mut engine, tree, &module_roots_for(source), &mut BTreeSet::new())?;
    Ok(engine)
}

fn empty_engine() -> Engine {
    Engine {
        env: BTreeMap::new(),
        functions: BTreeMap::new(),
        queries: BTreeMap::new(),
        objects: BTreeMap::new(),
        work: 0,
        work_limit: DEFAULT_WORK,
        memo: BTreeMap::new(),
        visit: 0,
        source_id: String::new(),
        entry: String::new(),
        source_text: String::new(),
        inputs: BTreeMap::new(),
        call_depth: 0,
        next_ref: 0,
        frames: Vec::new(),
        scopes: BTreeSet::new(),
        resume_frames: Vec::new(),
    }
}

fn install_local_items(engine: &mut Engine, tree: &SyntaxTree) -> Result<(), ConstructorError> {
    admit_constructor_surface(tree)?;
    for item in &tree.items {
        if let Item::Declaration(decl) = item {
            install_declaration(engine, decl, None)?;
        }
    }
    Ok(())
}

fn install_declaration(
    engine: &mut Engine,
    decl: &Declaration,
    alias: Option<String>,
) -> Result<(), ConstructorError> {
    let name = alias.unwrap_or_else(|| decl.name.clone());
    match decl.as_kind.as_str() {
        "function" => {
            let outputs = section_fields(decl, "outputs");
            let typed_inputs = section_typed_fields(decl, "inputs");
            engine.functions.insert(
                name,
                FnDecl {
                    inputs: typed_inputs.iter().map(|(name, _)| name.clone()).collect(),
                    input_types: typed_inputs.into_iter().map(|(_, ty)| ty).collect(),
                    output: outputs.first().cloned(),
                    output_types: section_typed_fields(decl, "outputs")
                        .into_iter()
                        .map(|(_, ty)| ty)
                        .collect(),
                    outputs,
                    defs: section_assigns(decl, "definitions"),
                    opaque: function_is_opaque(decl),
                },
            );
        }
        "query" => {
            engine.queries.insert(
                name,
                QueryDecl {
                    inputs: section_fields(decl, "inputs"),
                    defs: section_assigns(decl, "definitions"),
                    form: section_text(decl, "answer", "form").unwrap_or_else(|| "value".into()),
                    method: section_text(decl, "using", "method"),
                    accept: section_text(decl, "answer", "accept"),
                },
            );
        }
        "object" => {
            engine.objects.insert(
                name,
                ObjectSchema {
                    kind: object_representation_kind(decl),
                    invariants: section_assigns(decl, "invariants"),
                },
            );
        }
        "feature" => {
            return Err(fault(
                "E-KIND-GONE",
                "declaration kind `feature` is not a core kind",
            ));
        }
        other => {
            return Err(fault(
                "E-KIND-GONE",
                format!("declaration kind `{other}` is not a core kind"),
            ));
        }
    }
    Ok(())
}

fn load_imports(
    engine: &mut Engine,
    tree: &SyntaxTree,
    roots: &[PathBuf],
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<(), ConstructorError> {
    for item in &tree.items {
        let Item::Use { path, tree: use_tree, .. } = item else {
            continue;
        };
        let file = resolve_module_path(path, roots)?;
        if visiting.contains(&file) {
            return Err(fault(
                "E-USE-ADMISSION",
                format!("cyclic import `{}`", path.join(".")),
            ));
        }
        visiting.insert(file.clone());
        let text = std::fs::read_to_string(&file).map_err(|err| {
            fault(
                "E-USE-ADMISSION",
                format!("cannot read {}: {err}", file.display()),
            )
        })?;
        let (imported, diagnostics) = emath_syntax::parse_str(&text);
        if diagnostics.has_errors() {
            visiting.remove(&file);
            return Err(fault(
                "E-USE-ADMISSION",
                format!("cannot parse {}: {diagnostics:?}", file.display()),
            ));
        }
        load_imports(engine, &imported, roots, visiting)?;
        install_imported(engine, &imported, path, use_tree)?;
        visiting.remove(&file);
    }
    Ok(())
}

fn install_imported(
    engine: &mut Engine,
    tree: &SyntaxTree,
    path: &[String],
    use_tree: &UseTree,
) -> Result<(), ConstructorError> {
    let prefix = path.join(".");
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if matches!(decl.as_kind.as_str(), "feature") {
            return Err(fault(
                "E-KIND-GONE",
                "declaration kind `feature` is not a core kind",
            ));
        }
        let selected = match use_tree {
            UseTree::All => true,
            UseTree::Named(names) if names.is_empty() => true,
            UseTree::Named(names) => names.iter().any(|(name, _)| name == &decl.name),
        };
        if !selected {
            // Still install under the original name so callees in the same
            // module can resolve private helpers.
            install_declaration(engine, decl, None)?;
            continue;
        }
        install_declaration(engine, decl, None)?;
        engine.functions.get(&decl.name).cloned().map(|decl_fn| {
            engine.functions.insert(format!("{prefix}.{}", decl.name), decl_fn);
        });
        engine.queries.get(&decl.name).cloned().map(|decl_q| {
            engine.queries.insert(format!("{prefix}.{}", decl.name), decl_q);
        });
        if let UseTree::Named(names) = use_tree {
            if let Some((_, Some(alias))) = names.iter().find(|(name, _)| name == &decl.name) {
                install_declaration(engine, decl, Some(alias.clone()))?;
            }
        }
    }
    Ok(())
}

/// Resolve `use fold.reduce` against `language/modules` search roots.
pub fn resolve_module_path(path: &[String], roots: &[PathBuf]) -> Result<PathBuf, ConstructorError> {
    let mut segs: Vec<String> = path.to_vec();
    if segs.first().map(String::as_str) == Some("language") {
        segs.remove(0);
    }
    if segs.first().map(String::as_str) == Some("modules") {
        segs.remove(0);
    }
    let rel = PathBuf::from_iter(segs.iter()).with_extension("emath");
    for root in roots {
        let candidate = root.join(&rel);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fault(
        "E-USE-ADMISSION",
        format!("unbound import `{}`", path.join(".")),
    ))
}

/// Search roots for ordinary modules: walk from a source file, cwd, and this crate.
pub fn module_roots_for(source: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut consider = |start: PathBuf| {
        let mut dir = start;
        for _ in 0..12 {
            let modules = dir.join("language").join("modules");
            if modules.is_dir() {
                roots.push(modules);
            }
            if !dir.pop() {
                break;
            }
        }
    };
    if let Some(path) = source {
        if let Some(parent) = path.parent() {
            consider(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        consider(cwd);
    }
    consider(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    roots.sort();
    roots.dedup();
    roots
}

pub fn is_constructor_checkpoint(text: &str) -> bool {
    text.lines().next() == Some(CHECKPOINT_SCHEMA)
        || text.contains(&format!("\"schema\": \"{CHECKPOINT_SCHEMA}\""))
}

impl Checkpoint {
    pub fn encode(&self) -> String {
        let mut out = String::new();
        out.push_str(CHECKPOINT_SCHEMA);
        out.push('\n');
        out.push_str(&format!("source_id={}\n", self.source_id));
        out.push_str(&format!("image={}\n", self.image));
        out.push_str(&format!("abi={}\n", self.abi));
        out.push_str(&format!("work={}\n", self.work));
        out.push_str(&format!("function={}\n", self.function));
        out.push_str(&format!("next_ref={}\n", self.next_ref));
        out.push_str(&format!("accounting={}\n", self.accounting));
        out.push_str(&format!("remaining={}\n", self.remaining));
        out.push_str(&format!("scope_count={}\n", self.scopes.len()));
        for id in &self.scopes {
            out.push_str(&format!("scope {id}\n"));
        }
        out.push_str(&format!("frame_count={}\n", self.frames.len()));
        for frame in &self.frames {
            out.push_str(&format!(
                "frame function={} pc={} next={} env_count={} kont_count={}\n",
                frame.function,
                frame.pc,
                frame.next,
                frame.env.len(),
                frame.kont.len()
            ));
            for (name, value) in &frame.env {
                out.push_str("env ");
                out.push_str(name);
                out.push(' ');
                encode_cvalue(value, &mut out);
                out.push('\n');
            }
            for kont in &frame.kont {
                encode_kont(kont, &mut out);
            }
        }
        out.push_str(&format!("input_count={}\n", self.inputs.len()));
        for (name, value) in &self.inputs {
            out.push_str("input ");
            out.push_str(name);
            out.push(' ');
            encode_cvalue(value, &mut out);
            out.push('\n');
        }
        out.push_str(&format!("memo_count={}\n", self.memo.len()));
        for (id, value) in &self.memo {
            out.push_str("memo ");
            out.push_str(id);
            out.push(' ');
            encode_cvalue(value, &mut out);
            out.push('\n');
        }
        out.push_str(&format!("source_len={}\n", self.source.len()));
        out.push_str(&self.source);
        out
    }

    pub fn decode(text: &str) -> Result<Self, ConstructorError> {
        let Some((header, rest)) = text.split_once('\n') else {
            return Err(fault("incompatible_checkpoint", "checkpoint is empty"));
        };
        if header.trim() != CHECKPOINT_SCHEMA {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint schema does not match this constructor layer",
            ));
        }
        let mut checkpoint = Checkpoint::default();
        let mut lines = rest.lines();
        while let Some(line) = lines.next() {
            if let Some(value) = line.strip_prefix("source_id=") {
                checkpoint.source_id = value.to_string();
            } else if let Some(value) = line.strip_prefix("image=") {
                checkpoint.image = value.to_string();
            } else if let Some(value) = line.strip_prefix("abi=") {
                checkpoint.abi = value.to_string();
            } else if let Some(value) = line.strip_prefix("work=") {
                checkpoint.work = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "checkpoint work is not an integer")
                })?;
            } else if let Some(value) = line.strip_prefix("function=") {
                checkpoint.function = value.to_string();
            } else if let Some(value) = line.strip_prefix("next_ref=") {
                checkpoint.next_ref = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "next_ref is not an integer")
                })?;
            } else if let Some(value) = line.strip_prefix("accounting=") {
                checkpoint.accounting = value.to_string();
            } else if let Some(value) = line.strip_prefix("remaining=") {
                checkpoint.remaining = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "remaining is not an integer")
                })?;
            } else if let Some(value) = line.strip_prefix("scope_count=") {
                let count: usize = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "scope_count is not an integer")
                })?;
                for _ in 0..count {
                    let Some(scope_line) = lines.next() else {
                        return Err(fault("incompatible_checkpoint", "missing checkpoint scope"));
                    };
                    let Some(id) = scope_line.strip_prefix("scope ") else {
                        return Err(fault("incompatible_checkpoint", "malformed checkpoint scope"));
                    };
                    let id = id.parse().map_err(|_| {
                        fault("incompatible_checkpoint", "scope id is not an integer")
                    })?;
                    checkpoint.scopes.insert(id);
                }
            } else if let Some(value) = line.strip_prefix("frame_count=") {
                let count: usize = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "frame_count is not an integer")
                })?;
                for _ in 0..count {
                    let Some(header) = lines.next() else {
                        return Err(fault("incompatible_checkpoint", "missing checkpoint frame"));
                    };
                    let mut function = String::new();
                    let mut pc = 0_u64;
                    let mut next = String::new();
                    let mut env_count = 0_usize;
                    let mut kont_count = 0_usize;
                    for part in header.split_whitespace() {
                        if let Some(name) = part.strip_prefix("function=") {
                            function = name.to_string();
                        } else if let Some(value) = part.strip_prefix("pc=") {
                            pc = value.parse().map_err(|_| {
                                fault("incompatible_checkpoint", "frame pc is not an integer")
                            })?;
                        } else if let Some(value) = part.strip_prefix("next=") {
                            next = value.to_string();
                        } else if let Some(value) = part.strip_prefix("env_count=") {
                            env_count = value.parse().map_err(|_| {
                                fault("incompatible_checkpoint", "env_count is not an integer")
                            })?;
                        } else if let Some(value) = part.strip_prefix("kont_count=") {
                            kont_count = value.parse().map_err(|_| {
                                fault("incompatible_checkpoint", "kont_count is not an integer")
                            })?;
                        }
                    }
                    let mut env = BTreeMap::new();
                    for _ in 0..env_count {
                        let Some(env_line) = lines.next() else {
                            return Err(fault("incompatible_checkpoint", "missing frame env"));
                        };
                        let Some(rest) = env_line.strip_prefix("env ") else {
                            return Err(fault("incompatible_checkpoint", "malformed frame env"));
                        };
                        let Some((name, encoded)) = rest.split_once(' ') else {
                            return Err(fault("incompatible_checkpoint", "malformed frame env"));
                        };
                        env.insert(name.to_string(), decode_cvalue(encoded)?);
                    }
                    let mut kont = Vec::new();
                    for _ in 0..kont_count {
                        kont.push(Box::new(decode_kont(&mut lines)?));
                    }
                    checkpoint.frames.push(ContinuationFrame {
                        function,
                        pc,
                        next,
                        env,
                        kont,
                    });
                }
            } else if let Some(value) = line.strip_prefix("input_count=") {
                let count: usize = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "input_count is not an integer")
                })?;
                for _ in 0..count {
                    let Some(input_line) = lines.next() else {
                        return Err(fault("incompatible_checkpoint", "missing checkpoint input"));
                    };
                    let Some(rest) = input_line.strip_prefix("input ") else {
                        return Err(fault("incompatible_checkpoint", "malformed checkpoint input"));
                    };
                    let Some((name, encoded)) = rest.split_once(' ') else {
                        return Err(fault("incompatible_checkpoint", "malformed checkpoint input"));
                    };
                    checkpoint
                        .inputs
                        .insert(name.to_string(), decode_cvalue(encoded)?);
                }
            } else if let Some(value) = line.strip_prefix("memo_count=") {
                let count: usize = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "memo_count is not an integer")
                })?;
                for _ in 0..count {
                    let Some(memo_line) = lines.next() else {
                        return Err(fault("incompatible_checkpoint", "missing checkpoint memo"));
                    };
                    let Some(rest) = memo_line.strip_prefix("memo ") else {
                        return Err(fault("incompatible_checkpoint", "malformed checkpoint memo"));
                    };
                    let Some((id, encoded)) = rest.split_once(' ') else {
                        return Err(fault("incompatible_checkpoint", "malformed checkpoint memo"));
                    };
                    if id.is_empty() || id.contains(' ') {
                        return Err(fault("incompatible_checkpoint", "malformed checkpoint memo"));
                    }
                    checkpoint.memo.insert(id.to_string(), decode_cvalue(encoded)?);
                }
            } else if let Some(value) = line.strip_prefix("source_len=") {
                let len: usize = value.parse().map_err(|_| {
                    fault("incompatible_checkpoint", "source_len is not an integer")
                })?;
                let Some((_, after)) = rest.split_once("source_len=") else {
                    return Err(fault("incompatible_checkpoint", "truncated checkpoint source"));
                };
                let Some((_, remainder)) = after.split_once('\n') else {
                    checkpoint.source = String::new();
                    break;
                };
                if remainder.len() < len {
                    return Err(fault("incompatible_checkpoint", "truncated checkpoint source"));
                }
                checkpoint.source = remainder[..len].to_string();
                break;
            }
        }
        if checkpoint.abi != CHECKPOINT_ABI {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint ABI does not match this constructor layer",
            ));
        }
        Ok(checkpoint)
    }
}

fn call_memo_key(name: &str, args: &[CValue]) -> Option<String> {
    let mut out = String::from("call:");
    compact_ident(name, &mut out);
    for arg in args {
        out.push(':');
        compact_key(arg, &mut out)?;
    }
    Some(out)
}

fn closure_memo_key(clos: &Closure, args: &[CValue]) -> Option<String> {
    let name = clos.recursive.as_deref().unwrap_or(clos.param.as_str());
    let mut out = call_memo_key(name, args)?;
    for (key, value) in &clos.env {
        if clos.recursive.as_deref() == Some(key.as_str()) {
            continue;
        }
        out.push(':');
        compact_ident(key, &mut out);
        out.push('=');
        compact_key(value, &mut out)?;
    }
    Some(out)
}

fn compact_ident(name: &str, out: &mut String) {
    for ch in name.chars() {
        if ch.is_ascii_whitespace() || ch == ':' || ch == '=' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
}

fn compact_key(value: &CValue, out: &mut String) -> Option<()> {
    match value {
        CValue::Bool(v) => {
            out.push_str(if *v { "Btrue" } else { "Bfalse" });
            Some(())
        }
        CValue::Int(n) => {
            out.push('I');
            out.push_str(&n.to_string());
            Some(())
        }
        CValue::Rat { num, den } => {
            out.push('R');
            out.push_str(&num.to_string());
            out.push('/');
            out.push_str(&den.to_string());
            Some(())
        }
        CValue::Float64(x) => {
            out.push('F');
            out.push_str(&x.to_bits().to_string());
            Some(())
        }
        CValue::Sequence(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                compact_key(item, out)?;
            }
            out.push(']');
            Some(())
        }
        CValue::Tuple(items) => {
            out.push('(');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                compact_key(item, out)?;
            }
            out.push(')');
            Some(())
        }
        CValue::Record { type_name, fields } => {
            out.push('{');
            compact_ident(type_name, out);
            for (name, field) in fields {
                out.push(',');
                compact_ident(name, out);
                out.push('=');
                compact_key(field, out)?;
            }
            out.push('}');
            Some(())
        }
        CValue::Variant {
            type_name,
            tag,
            fields,
        } => {
            compact_ident(type_name, out);
            out.push('.');
            compact_ident(tag, out);
            out.push('(');
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                compact_key(field, out)?;
            }
            out.push(')');
            Some(())
        }
        CValue::Unit => {
            out.push_str("Unit");
            Some(())
        }
        CValue::Absent => {
            out.push_str("Absent");
            Some(())
        }
        CValue::Closure(_) | CValue::Code(_) | CValue::Receipt(_) => None,
    }
}

fn encode_kont(kont: &Kont, out: &mut String) {
    match kont {
        Kont::BinLeft { op, left, right } => {
            out.push_str("kont BinLeft ");
            out.push_str(bin_name(*op));
            out.push('\n');
            out.push_str("expr ");
            encode_expr(left, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(right, out);
            out.push('\n');
        }
        Kont::BinRight { op, left, right } => {
            out.push_str("kont BinRight ");
            out.push_str(bin_name(*op));
            out.push('\n');
            out.push_str("value ");
            encode_cvalue(left, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(right, out);
            out.push('\n');
        }
        Kont::IfAfterCond {
            condition,
            then_value,
            else_value,
        } => {
            out.push_str("kont IfAfterCond\n");
            out.push_str("expr ");
            encode_expr(condition, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(then_value, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(else_value, out);
            out.push('\n');
        }
        Kont::IfThen { then_value } => {
            out.push_str("kont IfThen\n");
            out.push_str("expr ");
            encode_expr(then_value, out);
            out.push('\n');
        }
        Kont::IfElse { else_value } => {
            out.push_str("kont IfElse\n");
            out.push_str("expr ");
            encode_expr(else_value, out);
            out.push('\n');
        }
        Kont::CallArgs {
            callee,
            done,
            rest,
        } => {
            out.push_str(&format!("kont CallArgs {} {}\n", done.len(), rest.len()));
            out.push_str("value ");
            encode_cvalue(callee, out);
            out.push('\n');
            for value in done {
                out.push_str("value ");
                encode_cvalue(value, out);
                out.push('\n');
            }
            for expr in rest {
                out.push_str("expr ");
                encode_expr(expr, out);
                out.push('\n');
            }
        }
        Kont::FnCall { name, done, rest } => {
            out.push_str(&format!("kont FnCall {name} {} {}\n", done.len(), rest.len()));
            for value in done {
                out.push_str("value ");
                encode_cvalue(value, out);
                out.push('\n');
            }
            for expr in rest {
                out.push_str("expr ");
                encode_expr(expr, out);
                out.push('\n');
            }
        }
        Kont::SeqItems {
            as_tuple,
            done,
            rest,
        } => {
            out.push_str(&format!(
                "kont SeqItems {} {} {}\n",
                if *as_tuple { "tuple" } else { "list" },
                done.len(),
                rest.len()
            ));
            for value in done {
                out.push_str("value ");
                encode_cvalue(value, out);
                out.push('\n');
            }
            for expr in rest {
                out.push_str("expr ");
                encode_expr(expr, out);
                out.push('\n');
            }
        }
        Kont::RecordFields {
            type_path,
            done,
            current,
            current_expr,
            rest,
        } => {
            out.push_str(&format!(
                "kont RecordFields {} {} {}\n",
                done.len(),
                rest.len(),
                type_path.join(".")
            ));
            out.push_str("name ");
            out.push_str(current);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(current_expr, out);
            out.push('\n');
            for (name, value) in done {
                out.push_str("name ");
                out.push_str(name);
                out.push('\n');
                out.push_str("value ");
                encode_cvalue(value, out);
                out.push('\n');
            }
            for (name, expr) in rest {
                out.push_str("name ");
                out.push_str(name);
                out.push('\n');
                out.push_str("expr ");
                encode_expr(expr, out);
                out.push('\n');
            }
        }
        Kont::IndexAfterSeq { seq, index } => {
            out.push_str("kont IndexAfterSeq\n");
            out.push_str("value ");
            encode_cvalue(seq, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(index, out);
            out.push('\n');
        }
        Kont::UnaryAfter { op, value } => {
            out.push_str("kont UnaryAfter ");
            out.push_str(un_name(*op));
            out.push('\n');
            out.push_str("expr ");
            encode_expr(value, out);
            out.push('\n');
        }
        Kont::ConsLeft { head, tail } => {
            out.push_str("kont ConsLeft\n");
            out.push_str("expr ");
            encode_expr(head, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(tail, out);
            out.push('\n');
        }
        Kont::ConsAfterHead { head, tail } => {
            out.push_str("kont ConsAfterHead\n");
            out.push_str("value ");
            encode_cvalue(head, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(tail, out);
            out.push('\n');
        }
        Kont::MatchWaiting {
            subject,
            arms,
            else_arm,
        } => {
            out.push_str(&format!("kont MatchWaiting {}\n", arms.len()));
            out.push_str("expr ");
            encode_expr(subject, out);
            out.push('\n');
            for (cond, value) in arms {
                out.push_str("expr ");
                encode_expr(cond, out);
                out.push('\n');
                out.push_str("expr ");
                encode_expr(value, out);
                out.push('\n');
            }
            out.push_str("expr ");
            encode_expr(else_arm, out);
            out.push('\n');
        }
        Kont::CasesArm {
            cond,
            value,
            rest,
            else_arm,
        } => {
            out.push_str(&format!("kont CasesArm {}\n", rest.len()));
            out.push_str("expr ");
            encode_expr(cond, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(value, out);
            out.push('\n');
            for (cond, value) in rest {
                out.push_str("expr ");
                encode_expr(cond, out);
                out.push('\n');
                out.push_str("expr ");
                encode_expr(value, out);
                out.push('\n');
            }
            out.push_str("expr ");
            encode_expr(else_arm, out);
            out.push('\n');
        }
        Kont::OpenAfterPacked {
            type_name,
            param,
            packed,
            body,
        } => {
            out.push_str("kont OpenAfterPacked\n");
            out.push_str("name ");
            out.push_str(type_name);
            out.push('\n');
            out.push_str("name ");
            out.push_str(param);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(packed, out);
            out.push('\n');
            out.push_str("expr ");
            encode_expr(body, out);
            out.push('\n');
        }
        Kont::EvalExpr { expr } => {
            out.push_str("kont EvalExpr\n");
            out.push_str("expr ");
            encode_expr(expr, out);
            out.push('\n');
        }
    }
}

fn decode_kont<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Result<Kont, ConstructorError> {
    let header = lines
        .next()
        .ok_or_else(|| fault("incompatible_checkpoint", "missing continuation slot"))?;
    let rest = header
        .strip_prefix("kont ")
        .ok_or_else(|| fault("incompatible_checkpoint", "malformed continuation slot"))?;
    let mut parts = rest.split_whitespace();
    let tag = parts
        .next()
        .ok_or_else(|| fault("incompatible_checkpoint", "continuation tag missing"))?;
    match tag {
        "BinLeft" => {
            let op = bin_op(parts.next().unwrap_or_default()).ok_or_else(|| {
                fault("incompatible_checkpoint", "unknown binary continuation")
            })?;
            Ok(Kont::BinLeft {
                op,
                left: Box::new(decode_expr_line(lines)?),
                right: Box::new(decode_expr_line(lines)?),
            })
        }
        "BinRight" => {
            let op = bin_op(parts.next().unwrap_or_default()).ok_or_else(|| {
                fault("incompatible_checkpoint", "unknown binary continuation")
            })?;
            Ok(Kont::BinRight {
                op,
                left: decode_value_line(lines)?,
                right: Box::new(decode_expr_line(lines)?),
            })
        }
        "IfAfterCond" => Ok(Kont::IfAfterCond {
            condition: Box::new(decode_expr_line(lines)?),
            then_value: Box::new(decode_expr_line(lines)?),
            else_value: Box::new(decode_expr_line(lines)?),
        }),
        "IfThen" => Ok(Kont::IfThen {
            then_value: Box::new(decode_expr_line(lines)?),
        }),
        "IfElse" => Ok(Kont::IfElse {
            else_value: Box::new(decode_expr_line(lines)?),
        }),
        "CallArgs" => {
            let done_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "call continuation arity")
            })?;
            let rest_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "call continuation arity")
            })?;
            let callee = decode_value_line(lines)?;
            let mut done = Vec::new();
            for _ in 0..done_count {
                done.push(decode_value_line(lines)?);
            }
            let mut rest = Vec::new();
            for _ in 0..rest_count {
                rest.push(decode_expr_line(lines)?);
            }
            Ok(Kont::CallArgs {
                callee,
                done,
                rest,
            })
        }
        "FnCall" => {
            let name = parts
                .next()
                .ok_or_else(|| fault("incompatible_checkpoint", "fn continuation name"))?
                .to_string();
            let done_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "fn continuation arity")
            })?;
            let rest_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "fn continuation arity")
            })?;
            let mut done = Vec::new();
            for _ in 0..done_count {
                done.push(decode_value_line(lines)?);
            }
            let mut rest = Vec::new();
            for _ in 0..rest_count {
                rest.push(decode_expr_line(lines)?);
            }
            Ok(Kont::FnCall { name, done, rest })
        }
        "SeqItems" => {
            let as_tuple = parts.next() == Some("tuple");
            let done_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "seq continuation arity")
            })?;
            let rest_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "seq continuation arity")
            })?;
            let mut done = Vec::new();
            for _ in 0..done_count {
                done.push(decode_value_line(lines)?);
            }
            let mut rest = Vec::new();
            for _ in 0..rest_count {
                rest.push(decode_expr_line(lines)?);
            }
            Ok(Kont::SeqItems {
                as_tuple,
                done,
                rest,
            })
        }
        "RecordFields" => {
            let done_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "record continuation arity")
            })?;
            let rest_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "record continuation arity")
            })?;
            let type_path = parts
                .next()
                .unwrap_or_default()
                .split('.')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect();
            let current = decode_name_line(lines)?;
            let current_expr = decode_expr_line(lines)?;
            let mut done = BTreeMap::new();
            for _ in 0..done_count {
                let name = decode_name_line(lines)?;
                done.insert(name, decode_value_line(lines)?);
            }
            let mut rest = Vec::new();
            for _ in 0..rest_count {
                rest.push((decode_name_line(lines)?, decode_expr_line(lines)?));
            }
            Ok(Kont::RecordFields {
                type_path,
                done,
                current,
                current_expr: Box::new(current_expr),
                rest,
            })
        }
        "IndexAfterSeq" => Ok(Kont::IndexAfterSeq {
            seq: decode_value_line(lines)?,
            index: Box::new(decode_expr_line(lines)?),
        }),
        "UnaryAfter" => {
            let op = un_op(parts.next().unwrap_or_default()).ok_or_else(|| {
                fault("incompatible_checkpoint", "unknown unary continuation")
            })?;
            Ok(Kont::UnaryAfter {
                op,
                value: Box::new(decode_expr_line(lines)?),
            })
        }
        "ConsLeft" => Ok(Kont::ConsLeft {
            head: Box::new(decode_expr_line(lines)?),
            tail: Box::new(decode_expr_line(lines)?),
        }),
        "ConsAfterHead" => Ok(Kont::ConsAfterHead {
            head: decode_value_line(lines)?,
            tail: Box::new(decode_expr_line(lines)?),
        }),
        "MatchWaiting" => {
            let arm_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "match continuation arity")
            })?;
            let subject = decode_expr_line(lines)?;
            let mut arms = Vec::new();
            for _ in 0..arm_count {
                arms.push((decode_expr_line(lines)?, decode_expr_line(lines)?));
            }
            Ok(Kont::MatchWaiting {
                subject: Box::new(subject),
                arms,
                else_arm: Box::new(decode_expr_line(lines)?),
            })
        }
        "CasesArm" => {
            let rest_count: usize = parts.next().unwrap_or("0").parse().map_err(|_| {
                fault("incompatible_checkpoint", "cases continuation arity")
            })?;
            let cond = decode_expr_line(lines)?;
            let value = decode_expr_line(lines)?;
            let mut rest = Vec::new();
            for _ in 0..rest_count {
                rest.push((decode_expr_line(lines)?, decode_expr_line(lines)?));
            }
            Ok(Kont::CasesArm {
                cond: Box::new(cond),
                value: Box::new(value),
                rest,
                else_arm: Box::new(decode_expr_line(lines)?),
            })
        }
        "OpenAfterPacked" => Ok(Kont::OpenAfterPacked {
            type_name: decode_name_line(lines)?,
            param: decode_name_line(lines)?,
            packed: Box::new(decode_expr_line(lines)?),
            body: Box::new(decode_expr_line(lines)?),
        }),
        "EvalExpr" => Ok(Kont::EvalExpr {
            expr: Box::new(decode_expr_line(lines)?),
        }),
        other => Err(fault(
            "incompatible_checkpoint",
            format!("unknown continuation `{other}`"),
        )),
    }
}

fn decode_name_line<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
) -> Result<String, ConstructorError> {
    let line = lines
        .next()
        .ok_or_else(|| fault("incompatible_checkpoint", "missing continuation name"))?;
    line.strip_prefix("name ")
        .map(str::to_string)
        .ok_or_else(|| fault("incompatible_checkpoint", "malformed continuation name"))
}

fn decode_expr_line<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
) -> Result<Expr, ConstructorError> {
    let line = lines
        .next()
        .ok_or_else(|| fault("incompatible_checkpoint", "missing continuation expression"))?;
    let encoded = line
        .strip_prefix("expr ")
        .ok_or_else(|| fault("incompatible_checkpoint", "malformed continuation expression"))?;
    decode_expr_cur(&mut Cursor::new(encoded))
}

fn decode_value_line<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
) -> Result<CValue, ConstructorError> {
    let line = lines
        .next()
        .ok_or_else(|| fault("incompatible_checkpoint", "missing continuation value"))?;
    let encoded = line
        .strip_prefix("value ")
        .ok_or_else(|| fault("incompatible_checkpoint", "malformed continuation value"))?;
    decode_cvalue(encoded)
}

fn encode_cvalue(value: &CValue, out: &mut String) {
    match value {
        CValue::Bool(v) => out.push_str(if *v { "B true" } else { "B false" }),
        CValue::Int(n) => {
            out.push_str("I ");
            out.push_str(&n.to_string());
        }
        CValue::Rat { num, den } => {
            out.push_str("R ");
            out.push_str(&num.to_string());
            out.push(' ');
            out.push_str(&den.to_string());
        }
        CValue::Float64(x) => {
            out.push_str("F ");
            out.push_str(&x.to_string());
        }
        CValue::Unit => out.push('U'),
        CValue::Absent => out.push('A'),
        CValue::Sequence(items) => {
            out.push_str("(seq");
            for item in items {
                out.push(' ');
                encode_cvalue(item, out);
            }
            out.push(')');
        }
        CValue::Tuple(items) => {
            out.push_str("(tup");
            for item in items {
                out.push(' ');
                encode_cvalue(item, out);
            }
            out.push(')');
        }
        CValue::Closure(clos) => {
            out.push_str("(clos ");
            encode_quoted(&clos.param, out);
            out.push(' ');
            encode_quoted(clos.recursive.as_deref().unwrap_or(""), out);
            out.push_str(" (env");
            for (name, value) in &clos.env {
                out.push(' ');
                encode_quoted(name, out);
                out.push(' ');
                encode_cvalue(value, out);
            }
            out.push_str(") ");
            encode_expr(&clos.body, out);
            out.push(')');
        }
        CValue::Code(code) => {
            out.push_str("(code ");
            encode_expr(&code.expr, out);
            out.push(')');
        }
        CValue::Record { type_name, fields } => {
            out.push_str("(rec ");
            encode_quoted(type_name, out);
            for (name, value) in fields {
                out.push(' ');
                encode_quoted(name, out);
                out.push(' ');
                encode_cvalue(value, out);
            }
            out.push(')');
        }
        CValue::Variant {
            type_name,
            tag,
            fields,
        } => {
            out.push_str("(var ");
            encode_quoted(type_name, out);
            out.push(' ');
            encode_quoted(tag, out);
            for value in fields {
                out.push(' ');
                encode_cvalue(value, out);
            }
            out.push(')');
        }
        CValue::Receipt(_) => out.push('X'),
    }
}

fn decode_cvalue(text: &str) -> Result<CValue, ConstructorError> {
    let text = text.trim();
    if text == "U" {
        return Ok(CValue::Unit);
    }
    if text == "A" {
        return Ok(CValue::Absent);
    }
    if text == "X" {
        return Err(fault(
            "incompatible_checkpoint",
            "checkpoint memo contains a non-serializable value; inspect is allowed, resume is not",
        ));
    }
    if let Some(rest) = text.strip_prefix("B ") {
        return Ok(CValue::Bool(rest == "true"));
    }
    if let Some(rest) = text.strip_prefix("I ") {
        let n = rest
            .parse()
            .map_err(|_| fault("incompatible_checkpoint", "invalid Int in checkpoint"))?;
        return Ok(CValue::Int(n));
    }
    if let Some(rest) = text.strip_prefix("R ") {
        let mut parts = rest.split_whitespace();
        let num = parts
            .next()
            .and_then(|part| part.parse().ok())
            .ok_or_else(|| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        let den = parts
            .next()
            .and_then(|part| part.parse().ok())
            .ok_or_else(|| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        return Ok(CValue::Rat { num, den });
    }
    if let Some(rest) = text.strip_prefix("F ") {
        let x = rest
            .parse()
            .map_err(|_| fault("incompatible_checkpoint", "invalid Float64 in checkpoint"))?;
        return Ok(CValue::Float64(x));
    }
    if let Some(rest) = text.strip_prefix("S ") {
        return decode_compound(rest, true);
    }
    if let Some(rest) = text.strip_prefix("T ") {
        return decode_compound(rest, false);
    }
    if text.starts_with('(') {
        return decode_structured(text);
    }
    Err(fault(
        "incompatible_checkpoint",
        format!("unknown checkpoint value `{text}`"),
    ))
}

fn encode_quoted(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn encode_expr(expr: &Expr, out: &mut String) {
    match &expr.kind {
        ExprKind::Int(text) => {
            out.push_str("(int ");
            encode_quoted(text, out);
            out.push(')');
        }
        ExprKind::Float(text) => {
            out.push_str("(float ");
            encode_quoted(text, out);
            out.push(')');
        }
        ExprKind::Bool(value) => {
            out.push_str(if *value { "(bool true)" } else { "(bool false)" });
        }
        ExprKind::Path { segments, .. } => {
            out.push_str("(path ");
            encode_quoted(&segments.join("."), out);
            out.push(')');
        }
        ExprKind::Binary { op, left, right } => {
            out.push_str("(bin ");
            out.push_str(bin_name(*op));
            out.push(' ');
            encode_expr(left, out);
            out.push(' ');
            encode_expr(right, out);
            out.push(')');
        }
        ExprKind::Unary { op, value } => {
            out.push_str("(un ");
            out.push_str(un_name(*op));
            out.push(' ');
            encode_expr(value, out);
            out.push(')');
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            out.push_str("(if ");
            encode_expr(condition, out);
            out.push(' ');
            encode_expr(then_value, out);
            out.push(' ');
            encode_expr(else_value, out);
            out.push(')');
        }
        ExprKind::Call { function, args } => {
            out.push_str("(call ");
            encode_expr(function, out);
            for arg in args {
                out.push(' ');
                encode_expr(arg, out);
            }
            out.push(')');
        }
        ExprKind::FunctionAbs {
            param,
            domain,
            body,
        } => {
            out.push_str("(abs ");
            encode_quoted(param, out);
            out.push(' ');
            encode_expr(domain, out);
            out.push(' ');
            encode_expr(body, out);
            out.push(')');
        }
        ExprKind::Recur { name, ty, body } => {
            out.push_str("(recur ");
            encode_quoted(name, out);
            out.push(' ');
            encode_expr(ty, out);
            out.push(' ');
            encode_expr(body, out);
            out.push(')');
        }
        ExprKind::Quote { body } => {
            out.push_str("(quote ");
            encode_expr(body, out);
            out.push(')');
        }
        ExprKind::QuoteBind {
            param,
            domain,
            body,
        } => {
            out.push_str("(qbind ");
            encode_quoted(param, out);
            out.push(' ');
            encode_expr(domain, out);
            out.push(' ');
            encode_expr(body, out);
            out.push(')');
        }
        ExprKind::List(items) => {
            out.push_str("(list");
            for item in items {
                out.push(' ');
                encode_expr(item, out);
            }
            out.push(')');
        }
        ExprKind::Tuple(items) => {
            out.push_str("(tuple");
            for item in items {
                out.push(' ');
                encode_expr(item, out);
            }
            out.push(')');
        }
        ExprKind::Index { value, indices } => {
            out.push_str("(idx ");
            encode_expr(value, out);
            for index in indices {
                out.push(' ');
                encode_expr(index, out);
            }
            out.push(')');
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            out.push_str("(cases ");
            match subject {
                Some(subject) => encode_expr(subject, out),
                None => out.push_str("(none)"),
            }
            for (cond, value) in arms {
                out.push_str(" (arm ");
                encode_expr(cond, out);
                out.push(' ');
                encode_expr(value, out);
                out.push(')');
            }
            out.push(' ');
            encode_expr(else_arm, out);
            out.push(')');
        }
        _ => out.push_str("(opaque)"),
    }
}

fn bin_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
        BinaryOp::Div => "div",
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::Lt => "lt",
        BinaryOp::Le => "le",
        BinaryOp::Gt => "gt",
        BinaryOp::Ge => "ge",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        _ => "other",
    }
}

fn un_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "neg",
        UnaryOp::Not => "not",
        UnaryOp::Pos => "pos",
    }
}

fn bin_op(name: &str) -> Option<BinaryOp> {
    Some(match name {
        "add" => BinaryOp::Add,
        "sub" => BinaryOp::Sub,
        "mul" => BinaryOp::Mul,
        "div" => BinaryOp::Div,
        "eq" => BinaryOp::Eq,
        "ne" => BinaryOp::Ne,
        "lt" => BinaryOp::Lt,
        "le" => BinaryOp::Le,
        "gt" => BinaryOp::Gt,
        "ge" => BinaryOp::Ge,
        "and" => BinaryOp::And,
        "or" => BinaryOp::Or,
        _ => return None,
    })
}

fn un_op(name: &str) -> Option<UnaryOp> {
    Some(match name {
        "neg" => UnaryOp::Neg,
        "not" => UnaryOp::Not,
        "pos" => UnaryOp::Pos,
        _ => return None,
    })
}

struct Cursor<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_whitespace())
        {
            self.pos += 1;
        }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn eat_char(&mut self, want: char) -> bool {
        self.skip_ws();
        if self.rest().starts_with(want) {
            self.pos += want.len_utf8();
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Result<&'a str, ConstructorError> {
        self.skip_ws();
        let start = self.pos;
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_alphabetic())
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(fault("incompatible_checkpoint", "expected identifier"));
        }
        Ok(&self.text[start..self.pos])
    }

    fn string(&mut self) -> Result<String, ConstructorError> {
        self.skip_ws();
        if !self.rest().starts_with('"') {
            return Err(fault("incompatible_checkpoint", "expected quoted string"));
        }
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.text.len() {
            let ch = self.text[self.pos..].chars().next().unwrap();
            self.pos += ch.len_utf8();
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let next = self.text[self.pos..].chars().next().ok_or_else(|| {
                        fault("incompatible_checkpoint", "truncated escape")
                    })?;
                    self.pos += next.len_utf8();
                    out.push(next);
                }
                other => out.push(other),
            }
        }
        Err(fault("incompatible_checkpoint", "unterminated string"))
    }
}

fn decode_structured(text: &str) -> Result<CValue, ConstructorError> {
    let mut cur = Cursor::new(text);
    decode_cvalue_cur(&mut cur)
}

fn decode_cvalue_cur(cur: &mut Cursor<'_>) -> Result<CValue, ConstructorError> {
    cur.skip_ws();
    if cur.eat_char('(') {
        let tag = cur.ident()?;
        let value = match tag {
            "clos" => {
                let param = cur.string()?;
                let recursive = cur.string()?;
                if !cur.eat_char('(') || cur.ident()? != "env" {
                    return Err(fault("incompatible_checkpoint", "closure env missing"));
                }
                let mut env = BTreeMap::new();
                while !cur.eat_char(')') {
                    let name = cur.string()?;
                    let value = decode_cvalue_cur(cur)?;
                    env.insert(name, value);
                }
                let body = decode_expr_cur(cur)?;
                if !cur.eat_char(')') {
                    return Err(fault("incompatible_checkpoint", "closure not closed"));
                }
                CValue::Closure(Box::new(Closure {
                    param,
                    body,
                    env,
                    recursive: if recursive.is_empty() {
                        None
                    } else {
                        Some(recursive)
                    },
                }))
            }
            "code" => {
                let expr = decode_expr_cur(cur)?;
                if !cur.eat_char(')') {
                    return Err(fault("incompatible_checkpoint", "code not closed"));
                }
                CValue::Code(Box::new(Code { expr }))
            }
            "rec" => {
                let type_name = cur.string()?;
                let mut fields = BTreeMap::new();
                while !cur.eat_char(')') {
                    let name = cur.string()?;
                    let value = decode_cvalue_cur(cur)?;
                    fields.insert(name, value);
                }
                CValue::Record { type_name, fields }
            }
            "var" => {
                let type_name = cur.string()?;
                let tag = cur.string()?;
                let mut fields = Vec::new();
                while !cur.eat_char(')') {
                    fields.push(decode_cvalue_cur(cur)?);
                }
                CValue::Variant {
                    type_name,
                    tag,
                    fields,
                }
            }
            "seq" => {
                let mut items = Vec::new();
                while !cur.eat_char(')') {
                    items.push(decode_cvalue_cur(cur)?);
                }
                CValue::Sequence(items)
            }
            "tup" => {
                let mut items = Vec::new();
                while !cur.eat_char(')') {
                    items.push(decode_cvalue_cur(cur)?);
                }
                CValue::Tuple(items)
            }
            _ => {
                return Err(fault(
                    "incompatible_checkpoint",
                    format!("unknown structured value `{tag}`"),
                ));
            }
        };
        return Ok(value);
    }
    decode_atom_cur(cur)
}

fn decode_atom_cur(cur: &mut Cursor<'_>) -> Result<CValue, ConstructorError> {
    cur.skip_ws();
    let rest = cur.rest();
    if rest.starts_with("B true") {
        cur.pos += 6;
        return Ok(CValue::Bool(true));
    }
    if rest.starts_with("B false") {
        cur.pos += 7;
        return Ok(CValue::Bool(false));
    }
    if let Some(after) = rest.strip_prefix("I ") {
        let (n, used) = take_token(after)?;
        cur.pos += 2 + used;
        let n = n
            .parse()
            .map_err(|_| fault("incompatible_checkpoint", "invalid Int in checkpoint"))?;
        return Ok(CValue::Int(n));
    }
    if let Some(after) = rest.strip_prefix("R ") {
        let (num, used_num) = take_token(after)?;
        let (den, used_den) = take_token(&after[used_num..])?;
        cur.pos += 2 + used_num + used_den;
        let num = num
            .parse()
            .map_err(|_| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        let den = den
            .parse()
            .map_err(|_| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        return Ok(CValue::Rat { num, den });
    }
    if let Some(after) = rest.strip_prefix("F ") {
        let (x, used) = take_token(after)?;
        cur.pos += 2 + used;
        let x = x
            .parse()
            .map_err(|_| fault("incompatible_checkpoint", "invalid Float64 in checkpoint"))?;
        return Ok(CValue::Float64(x));
    }
    if rest.starts_with('U') && token_boundary(rest, 1) {
        cur.pos += 1;
        return Ok(CValue::Unit);
    }
    if rest.starts_with('A') && token_boundary(rest, 1) {
        cur.pos += 1;
        return Ok(CValue::Absent);
    }
    if rest.starts_with('X') && token_boundary(rest, 1) {
        return Err(fault(
            "incompatible_checkpoint",
            "checkpoint memo contains a non-serializable value; inspect is allowed, resume is not",
        ));
    }
    Err(fault(
        "incompatible_checkpoint",
        format!("unknown checkpoint value `{}`", rest.chars().take(32).collect::<String>()),
    ))
}

fn token_boundary(text: &str, index: usize) -> bool {
    text.len() == index
        || text
            .as_bytes()
            .get(index)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b')')
}

fn take_token(text: &str) -> Result<(&str, usize), ConstructorError> {
    let trimmed = text.trim_start();
    let pad = text.len() - trimmed.len();
    let end = trimmed
        .find(|ch: char| ch.is_ascii_whitespace() || ch == ')')
        .unwrap_or(trimmed.len());
    if end == 0 {
        return Err(fault("incompatible_checkpoint", "missing token"));
    }
    Ok((&trimmed[..end], pad + end))
}

fn decode_expr_cur(cur: &mut Cursor<'_>) -> Result<Expr, ConstructorError> {
    if !cur.eat_char('(') {
        return Err(fault("incompatible_checkpoint", "expected expression"));
    }
    let tag = cur.ident()?;
    let kind = match tag {
        "int" => ExprKind::Int(cur.string()?),
        "float" => ExprKind::Float(cur.string()?),
        "bool" => {
            let name = cur.ident()?;
            ExprKind::Bool(name == "true")
        }
        "path" => ExprKind::Path {
            segments: cur.string()?.split('.').map(str::to_string).collect(),
            generics: None,
        },
        "bin" => {
            let op = bin_op(cur.ident()?).ok_or_else(|| {
                fault("incompatible_checkpoint", "unknown binary operator")
            })?;
            ExprKind::Binary {
                op,
                left: Box::new(decode_expr_cur(cur)?),
                right: Box::new(decode_expr_cur(cur)?),
            }
        }
        "un" => {
            let op = un_op(cur.ident()?).ok_or_else(|| {
                fault("incompatible_checkpoint", "unknown unary operator")
            })?;
            ExprKind::Unary {
                op,
                value: Box::new(decode_expr_cur(cur)?),
            }
        }
        "if" => ExprKind::If {
            condition: Box::new(decode_expr_cur(cur)?),
            then_value: Box::new(decode_expr_cur(cur)?),
            else_value: Box::new(decode_expr_cur(cur)?),
        },
        "call" => {
            let function = decode_expr_cur(cur)?;
            let mut args = Vec::new();
            while !cur.eat_char(')') {
                args.push(decode_expr_cur(cur)?);
            }
            return Ok(dummy_expr(ExprKind::Call {
                function: Box::new(function),
                args,
            }));
        }
        "abs" => ExprKind::FunctionAbs {
            param: cur.string()?,
            domain: Box::new(decode_expr_cur(cur)?),
            body: Box::new(decode_expr_cur(cur)?),
        },
        "recur" => ExprKind::Recur {
            name: cur.string()?,
            ty: Box::new(decode_expr_cur(cur)?),
            body: Box::new(decode_expr_cur(cur)?),
        },
        "quote" => ExprKind::Quote {
            body: Box::new(decode_expr_cur(cur)?),
        },
        "qbind" => ExprKind::QuoteBind {
            param: cur.string()?,
            domain: Box::new(decode_expr_cur(cur)?),
            body: Box::new(decode_expr_cur(cur)?),
        },
        "list" => {
            let mut items = Vec::new();
            while !cur.eat_char(')') {
                items.push(decode_expr_cur(cur)?);
            }
            return Ok(dummy_expr(ExprKind::List(items)));
        }
        "tuple" => {
            let mut items = Vec::new();
            while !cur.eat_char(')') {
                items.push(decode_expr_cur(cur)?);
            }
            return Ok(dummy_expr(ExprKind::Tuple(items)));
        }
        "idx" => {
            let value = decode_expr_cur(cur)?;
            let mut indices = Vec::new();
            while !cur.eat_char(')') {
                indices.push(decode_expr_cur(cur)?);
            }
            return Ok(dummy_expr(ExprKind::Index {
                value: Box::new(value),
                indices,
            }));
        }
        "cases" => {
            let subject = if cur.rest().trim_start().starts_with("(none)") {
                cur.skip_ws();
                cur.pos += "(none)".len();
                None
            } else {
                Some(Box::new(decode_expr_cur(cur)?))
            };
            let mut arms = Vec::new();
            loop {
                cur.skip_ws();
                if cur.rest().starts_with("(arm ") {
                    cur.pos += "(arm ".len();
                    let cond = decode_expr_cur(cur)?;
                    let value = decode_expr_cur(cur)?;
                    if !cur.eat_char(')') {
                        return Err(fault("incompatible_checkpoint", "arm not closed"));
                    }
                    arms.push((cond, value));
                } else {
                    break;
                }
            }
            let else_arm = Box::new(decode_expr_cur(cur)?);
            if !cur.eat_char(')') {
                return Err(fault("incompatible_checkpoint", "cases not closed"));
            }
            return Ok(dummy_expr(ExprKind::Cases {
                subject,
                arms,
                else_arm,
            }));
        }
        "none" => {
            if !cur.eat_char(')') {
                return Err(fault("incompatible_checkpoint", "none not closed"));
            }
            return Ok(dummy_expr(ExprKind::Tuple(Vec::new())));
        }
        "opaque" => {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint memo contains a non-serializable expression; inspect is allowed, resume is not",
            ));
        }
        other => {
            return Err(fault(
                "incompatible_checkpoint",
                format!("unknown expression `{other}`"),
            ));
        }
    };
    if !cur.eat_char(')') {
        return Err(fault("incompatible_checkpoint", "expression not closed"));
    }
    Ok(dummy_expr(kind))
}

fn decode_compound(text: &str, sequence: bool) -> Result<CValue, ConstructorError> {
    let mut parts = text.splitn(2, ';');
    let count: usize = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| fault("incompatible_checkpoint", "invalid compound count"))?;
    let mut items = Vec::new();
    if count == 0 {
        return Ok(if sequence {
            CValue::Sequence(items)
        } else {
            CValue::Tuple(items)
        });
    }
    let rest = parts
        .next()
        .ok_or_else(|| fault("incompatible_checkpoint", "truncated compound value"))?;
    for encoded in split_encoded(rest, count)? {
        items.push(decode_cvalue(&encoded)?);
    }
    Ok(if sequence {
        CValue::Sequence(items)
    } else {
        CValue::Tuple(items)
    })
}

fn split_encoded(text: &str, count: usize) -> Result<Vec<String>, ConstructorError> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    for ch in text.chars() {
        if ch == ';' && depth == 0 {
            items.push(std::mem::take(&mut current));
            continue;
        }
        if ch == ';' {
            depth -= 1;
        }
        if matches!(ch, 'S' | 'T') {
            // counts are encoded as `S n;...` — depth is handled by ';' only
        }
        current.push(ch);
        let _ = depth;
    }
    if !current.is_empty() {
        items.push(current);
    }
    if items.len() != count {
        return Err(fault(
            "incompatible_checkpoint",
            format!("compound expected {count} items, found {}", items.len()),
        ));
    }
    Ok(items)
}

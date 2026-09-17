use super::*;
use super::prelude::*;

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

pub(super) fn call_memo_key(name: &str, args: &[CValue]) -> Option<String> {
    let mut out = String::from("call:");
    compact_ident(name, &mut out);
    for arg in args {
        out.push(':');
        compact_key(arg, &mut out)?;
    }
    Some(out)
}

pub(super) fn closure_memo_key(clos: &Closure, args: &[CValue]) -> Option<String> {
    let name = clos.recursive.as_deref().unwrap_or(clos.param.as_str());
    let mut out = call_memo_key(name, args)?;
    out.push_str(":body=");
    encode_expr(&clos.body, &mut out);
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

pub(super) fn compact_ident(name: &str, out: &mut String) {
    for ch in name.chars() {
        if ch.is_ascii_whitespace() || ch == ':' || ch == '=' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
}

pub(super) fn compact_key(value: &CValue, out: &mut String) -> Option<()> {
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

pub(super) fn encode_kont(kont: &Kont, out: &mut String) {
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

pub(super) fn decode_kont<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Result<Kont, ConstructorError> {
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

pub(super) fn decode_name_line<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
) -> Result<String, ConstructorError> {
    let line = lines
        .next()
        .ok_or_else(|| fault("incompatible_checkpoint", "missing continuation name"))?;
    line.strip_prefix("name ")
        .map(str::to_string)
        .ok_or_else(|| fault("incompatible_checkpoint", "malformed continuation name"))
}

pub(super) fn decode_expr_line<'a>(
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

pub(super) fn decode_value_line<'a>(
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

pub(super) fn encode_cvalue(value: &CValue, out: &mut String) {
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

pub(super) fn decode_cvalue(text: &str) -> Result<CValue, ConstructorError> {
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
        let n = ExactInt::parse(rest)
            .map_err(|_| fault("incompatible_checkpoint", "invalid Int in checkpoint"))?;
        return Ok(CValue::Int(n));
    }
    if let Some(rest) = text.strip_prefix("R ") {
        let mut parts = rest.split_whitespace();
        let num = parts
            .next()
            .and_then(|part| ExactInt::parse(part).ok())
            .ok_or_else(|| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        let den = parts
            .next()
            .and_then(|part| ExactInt::parse(part).ok())
            .ok_or_else(|| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        return CValue::Rat { num, den }.canon();
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

pub(super) fn encode_quoted(text: &str, out: &mut String) {
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

pub(super) fn encode_expr(expr: &Expr, out: &mut String) {
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
        ExprKind::Rational { numer, denom } => {
            out.push_str("(rat ");
            encode_quoted(numer, out);
            out.push(' ');
            encode_quoted(denom, out);
            out.push(')');
        }
        ExprKind::Str(text) => {
            out.push_str("(str ");
            encode_quoted(text, out);
            out.push(')');
        }
        ExprKind::Record { type_path, fields } => {
            out.push_str("(record ");
            encode_quoted(&type_path.join("."), out);
            for (name, value) in fields {
                out.push(' ');
                encode_quoted(name, out);
                out.push('=');
                encode_expr(value, out);
            }
            out.push(')');
        }
        ExprKind::SequenceCons { head, tail } => {
            out.push_str("(cons ");
            encode_expr(head, out);
            out.push(' ');
            encode_expr(tail, out);
            out.push(')');
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            out.push_str("(range ");
            match start {
                Some(start) => encode_expr(start, out),
                None => out.push_str("(none)"),
            }
            out.push(' ');
            match end {
                Some(end) => encode_expr(end, out),
                None => out.push_str("(none)"),
            }
            out.push_str(if *inclusive { " inclusive)" } else { " exclusive)" });
        }
        // Residual kinds cannot be evaluated by the constructor layer, so
        // they can only appear inside unevaluated branches; the Debug
        // payload still distinguishes structurally different bodies so
        // memo keys never collide on `(opaque)` alone.
        other => {
            out.push_str("(opaque ");
            out.push_str(&format!("{:?}", other));
            out.push(')');
        }
    }
}

pub(super) fn bin_name(op: BinaryOp) -> &'static str {
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

pub(super) fn un_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "neg",
        UnaryOp::Not => "not",
        UnaryOp::Pos => "pos",
    }
}

pub(super) fn bin_op(name: &str) -> Option<BinaryOp> {
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

pub(super) fn un_op(name: &str) -> Option<UnaryOp> {
    Some(match name {
        "neg" => UnaryOp::Neg,
        "not" => UnaryOp::Not,
        "pos" => UnaryOp::Pos,
        _ => return None,
    })
}

pub(super) struct Cursor<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    pub(super) fn skip_ws(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_whitespace())
        {
            self.pos += 1;
        }
    }

    pub(super) fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    pub(super) fn eat_char(&mut self, want: char) -> bool {
        self.skip_ws();
        if self.rest().starts_with(want) {
            self.pos += want.len_utf8();
            true
        } else {
            false
        }
    }

    pub(super) fn ident(&mut self) -> Result<&'a str, ConstructorError> {
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

    pub(super) fn string(&mut self) -> Result<String, ConstructorError> {
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

pub(super) fn decode_structured(text: &str) -> Result<CValue, ConstructorError> {
    let mut cur = Cursor::new(text);
    decode_cvalue_cur(&mut cur)
}

pub(super) fn decode_cvalue_cur(cur: &mut Cursor<'_>) -> Result<CValue, ConstructorError> {
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
                CValue::Code(Box::new(Code { expr, deps: BTreeMap::new() }))
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

pub(super) fn decode_atom_cur(cur: &mut Cursor<'_>) -> Result<CValue, ConstructorError> {
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
        let n = ExactInt::parse(n)
            .map_err(|_| fault("incompatible_checkpoint", "invalid Int in checkpoint"))?;
        return Ok(CValue::Int(n));
    }
    if let Some(after) = rest.strip_prefix("R ") {
        let (num, used_num) = take_token(after)?;
        let (den, used_den) = take_token(&after[used_num..])?;
        cur.pos += 2 + used_num + used_den;
        let num = ExactInt::parse(num)
            .map_err(|_| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        let den = ExactInt::parse(den)
            .map_err(|_| fault("incompatible_checkpoint", "invalid Rat in checkpoint"))?;
        return CValue::Rat { num, den }.canon();
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

pub(super) fn token_boundary(text: &str, index: usize) -> bool {
    text.len() == index
        || text
            .as_bytes()
            .get(index)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b')')
}

pub(super) fn take_token(text: &str) -> Result<(&str, usize), ConstructorError> {
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

pub(super) fn decode_expr_cur(cur: &mut Cursor<'_>) -> Result<Expr, ConstructorError> {
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

pub(super) fn decode_compound(text: &str, sequence: bool) -> Result<CValue, ConstructorError> {
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

pub(super) fn split_encoded(text: &str, count: usize) -> Result<Vec<String>, ConstructorError> {
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


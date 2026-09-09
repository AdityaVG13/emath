use super::*;

pub(super) fn write_nested_programs(out: &mut String, op: &EmirOp, indent: usize) {
    match op {
        EmirOp::Iterate { body, stop, .. } => {
            if let Some(stop) = stop { stop.write_print(out, indent); }
            body.write_print(out, indent);
        }
        EmirOp::Fold { body, .. } | EmirOp::Collect { body, .. }
        | EmirOp::CallFrame { body, .. } | EmirOp::ProgramLiteral { body, .. } => body.write_print(out, indent),
        EmirOp::Branch { then_body, else_body, .. } => {
            then_body.write_print(out, indent);
            else_body.write_print(out, indent);
        }
        _ => {}
    }
}

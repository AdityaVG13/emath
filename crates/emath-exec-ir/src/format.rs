use super::*;

pub(super) fn write_nested_programs(out: &mut String, op: &EmirOp, indent: usize) {
    if let EmirOp::Fold { body, .. } = op {
        body.write_print(out, indent);
    }
}

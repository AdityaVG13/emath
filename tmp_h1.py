/// Evaluate already-shaped goal arguments through the authored reference
/// program installed for `capability`. Refusals surface under their authored
/// codes; any other evaluation fault is reported verbatim, the same way a
/// direct capability invocation surfaces it.
fn apply_authored(capability: &str, args: &[Value]) -> Result<Value, String> {
    let mut ops = Vec::with_capacity(args.len() + 1);
    for index in 0..args.len() {
        let register = u16::try_from(index).map_err(|_| {
            "E-TYPE-012: program-expression-goal passes too many arguments".to_string()
        })?;
        ops.push((EmirOp::LoadInput(register), Span::default()));
    }
    let result = u16::try_from(args.len()).map_err(|_| {
        "E-TYPE-012: program-expression-goal passes too many arguments".to_string()
    })?;
    ops.push((
        EmirOp::ApplyCapability {
            capability: capability.to_string(),
            class: CellClass::Pure,
            args: (0..args.len() as u32).map(EmirValue).collect(),
        },
        Span::default(),
    ));
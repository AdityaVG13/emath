    match evaluate(
        &EmirProgram {
            ops,
            result: EmirValue(result),
            input_count: result,
            state_count: 0,
            domain_obligations: Vec::new(),
        },
        args,
        &[],
    ) {
        Ok(value) => Ok(value),
        Err(EvalFault::CapabilityRefused { code, .. }) => Err(code),
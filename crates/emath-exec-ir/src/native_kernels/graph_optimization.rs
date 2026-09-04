//! Domain-neutral adapters for finite matrix, traversal, optimization, and
//! finite-profile kernels. Mathematical names and authority remain in capsules.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Descriptors to append to the immutable native-kernel registry.
///
/// Determinism is inherited from the underlying kernels: ascending index scans,
/// lowest-index graph ties, Bland simplex pivots, stable carrier order for
/// certificates, and complete ascending tie sets for game best responses.
pub const KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "adjacency-reachability",
        signature: "(Matrix<Float64>,Float64)->Vector<Float64>",
        arity: 2,
        handler: adjacency_reachability,
    },
    NativeKernel {
        kernel_id: "adjacency-breadth-order",
        signature: "(Matrix<Float64>,Float64)->Vector<Float64>",
        arity: 2,
        handler: adjacency_breadth_order,
    },
    NativeKernel {
        kernel_id: "nonnegative-shortest-path",
        signature: "(Matrix<Float64>,Float64)->Vector<Float64>",
        arity: 2,
        handler: nonnegative_shortest_path,
    },
    NativeKernel {
        kernel_id: "row-nonzero-counts",
        signature: "(Matrix<Float64>)->Vector<Float64>",
        arity: 1,
        handler: row_nonzero_counts,
    },
    NativeKernel {
        kernel_id: "degree-minus-adjacency",
        signature: "(Matrix<Float64>)->Matrix<Float64>",
        arity: 1,
        handler: degree_minus_adjacency,
    },
    NativeKernel {
        kernel_id: "transpose-average",
        signature: "(Matrix<Float64>)->Matrix<Float64>",
        arity: 1,
        handler: transpose_average,
    },
    NativeKernel {
        kernel_id: "relaxation-shortest-path",
        signature: "(Matrix<Float64>,Float64)->Vector<Float64>",
        arity: 2,
        handler: relaxation_shortest_path,
    },
    NativeKernel {
        kernel_id: "dense-to-coordinate-stream",
        signature: "(Matrix<Float64>)->Vector<Float64>",
        arity: 1,
        handler: dense_to_coordinate_stream,
    },
    NativeKernel {
        kernel_id: "coordinate-stream-to-dense",
        signature: "(Float64,Vector<Float64>)->Matrix<Float64>",
        arity: 2,
        handler: coordinate_stream_to_dense,
    },
    NativeKernel {
        kernel_id: "bland-simplex-minimize",
        signature: "(Matrix<Float64>,Vector<Float64>,Vector<Float64>)->Vector<Float64>",
        arity: 3,
        handler: bland_simplex_minimize,
    },
    NativeKernel {
        kernel_id: "nondominated-mask",
        signature: "(Matrix<Float64>)->Vector<Float64>",
        arity: 1,
        handler: nondominated_mask,
    },
    NativeKernel {
        kernel_id: "unilateral-deviation-check",
        signature: "(Matrix<Float64>,Matrix<Float64>,Float64,Float64)->Bool",
        arity: 4,
        handler: unilateral_deviation_check,
    },
    NativeKernel {
        kernel_id: "support-best-response-check",
        signature: "(Matrix<Float64>,Matrix<Float64>,Vector<Float64>,Vector<Float64>)->Bool",
        arity: 4,
        handler: support_best_response_check,
    },
    NativeKernel {
        kernel_id: "column-maximizer-set",
        signature: "(Matrix<Float64>,Float64)->Vector<Float64>",
        arity: 2,
        handler: column_maximizer_set,
    },
];

fn matrix(value: &Value) -> Result<(usize, usize, &[f64]), String> {
    match value {
        Value::Matrix { rows, cols, data } => Ok((*rows, *cols, data)),
        _ => Err("E-TYPE-012: kernel argument must be Matrix<Float64>".to_string()),
    }
}

fn vector(value: &Value) -> Result<&[f64], String> {
    match value {
        Value::Vector(entries) => Ok(entries),
        _ => Err("E-TYPE-012: kernel argument must be Vector<Float64>".to_string()),
    }
}

fn scalar(value: &Value) -> Result<f64, String> {
    match value {
        Value::F64(value) => Ok(*value),
        Value::I64(value) => Ok(*value as f64),
        _ => Err("E-TYPE-012: kernel argument must be scalar".to_string()),
    }
}

fn index(value: &Value, bound: usize, code: &str) -> Result<usize, String> {
    let value = scalar(value)?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value >= bound as f64 {
        return Err(code.to_string());
    }
    Ok(value as usize)
}

fn graph_error(error: emath_rt::DenseCarrierError) -> String {
    error.code().to_string()
}

fn adjacency_reachability(args: &[Value]) -> Result<Value, String> {
    let [adjacency, source] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    let source = index(source, rows, "E-GRAPH-003")?;
    emath_rt::reachable_mask(data, rows, cols, source)
        .map(Value::Vector)
        .map_err(graph_error)
}

fn adjacency_breadth_order(args: &[Value]) -> Result<Value, String> {
    let [adjacency, source] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    let source = index(source, rows, "E-GRAPH-003")?;
    emath_rt::breadth_order(data, rows, cols, source)
        .map(Value::Vector)
        .map_err(graph_error)
}

fn nonnegative_shortest_path(args: &[Value]) -> Result<Value, String> {
    let [adjacency, source] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    let source = index(source, rows, "E-GRAPH-003")?;
    emath_rt::nonnegative_shortest_path(data, rows, cols, source)
        .map(Value::Vector)
        .map_err(graph_error)
}

fn row_nonzero_counts(args: &[Value]) -> Result<Value, String> {
    let [adjacency] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    emath_rt::row_nonzero_counts(data, rows, cols)
        .map(Value::Vector)
        .map_err(graph_error)
}

fn degree_minus_adjacency(args: &[Value]) -> Result<Value, String> {
    let [adjacency] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    let data = emath_rt::degree_minus_carrier(data, rows, cols).map_err(graph_error)?;
    Ok(Value::Matrix { rows, cols, data })
}

fn transpose_average(args: &[Value]) -> Result<Value, String> {
    let [adjacency] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    let data = emath_rt::transpose_average(data, rows, cols).map_err(graph_error)?;
    Ok(Value::Matrix { rows, cols, data })
}

fn relaxation_shortest_path(args: &[Value]) -> Result<Value, String> {
    let [adjacency, source] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    let source = index(source, rows, "E-GRAPH-003")?;
    emath_rt::relaxation_shortest_path(data, rows, cols, source)
        .map(Value::Vector)
        .map_err(graph_error)
}

fn dense_to_coordinate_stream(args: &[Value]) -> Result<Value, String> {
    let [adjacency] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(adjacency)?;
    emath_rt::dense_to_coordinate_stream(data, rows, cols)
        .map(Value::Vector)
        .map_err(graph_error)
}

fn coordinate_stream_to_dense(args: &[Value]) -> Result<Value, String> {
    let [side, triplets] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let side = scalar(side)?;
    let data =
        emath_rt::coordinate_stream_to_dense(side, vector(triplets)?).map_err(graph_error)?;
    let extent = (data.len() as f64).sqrt() as usize;
    Ok(Value::Matrix {
        rows: extent,
        cols: extent,
        data,
    })
}

fn bland_simplex_minimize(args: &[Value]) -> Result<Value, String> {
    let [constraints, bounds, objective] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(constraints)?;
    emath_rt::constrained_linear_minimize(data, rows, cols, vector(bounds)?, vector(objective)?)
        .map(Value::Vector)
        .map_err(|error| error.code().to_string())
}

fn nondominated_mask(args: &[Value]) -> Result<Value, String> {
    let [points] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, data) = matrix(points)?;
    emath_rt::nondominated_mask(data, rows, cols)
        .map(Value::Vector)
        .map_err(|error| error.code().to_string())
}

fn game_matrices<'a>(
    row: &'a Value,
    column: &'a Value,
) -> Result<(usize, usize, &'a [f64], &'a [f64]), String> {
    let (rows, cols, row_data) = matrix(row)?;
    let (column_rows, column_cols, column_data) = matrix(column)?;
    if rows == 0
        || cols == 0
        || rows != column_rows
        || cols != column_cols
        || row_data.len() != rows * cols
        || column_data.len() != rows * cols
    {
        return Err(
            "E-GAME-001: payoff matrices must be non-empty and share one rectangular shape"
                .to_string(),
        );
    }
    if row_data
        .iter()
        .chain(column_data)
        .any(|value| !value.is_finite())
    {
        return Err("E-GAME-002: payoff entries must be finite".to_string());
    }
    Ok((rows, cols, row_data, column_data))
}

fn unilateral_deviation_check(args: &[Value]) -> Result<Value, String> {
    let [row_payoffs, column_payoffs, row, column] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, row_payoffs, column_payoffs) = game_matrices(row_payoffs, column_payoffs)?;
    let row = index(
        row,
        rows,
        "E-GAME-003: claimed profile is outside the finite strategy carrier",
    )?;
    let column = index(
        column,
        cols,
        "E-GAME-003: claimed profile is outside the finite strategy carrier",
    )?;
    let row_utility = row_payoffs[row * cols + column];
    let column_utility = column_payoffs[row * cols + column];
    let row_stable =
        (0..rows).all(|alternative| row_payoffs[alternative * cols + column] <= row_utility);
    let column_stable =
        (0..cols).all(|alternative| column_payoffs[row * cols + alternative] <= column_utility);
    Ok(Value::Bool(row_stable && column_stable))
}

fn distribution<'a>(value: &'a Value, expected: usize) -> Result<&'a [f64], String> {
    let weights = vector(value)?;
    if weights.len() != expected
        || weights.is_empty()
        || weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight < 0.0)
    {
        return Err(
            "E-GAME-004: mixed strategy must be a finite nonnegative vector of the carrier extent"
                .to_string(),
        );
    }
    let mass: f64 = weights.iter().sum();
    if (mass - 1.0).abs() > 1e-9 {
        return Err(
            "E-GAME-005: mixed strategy mass must be one and is never renormalized".to_string(),
        );
    }
    Ok(weights)
}

fn support_best_response_check(args: &[Value]) -> Result<Value, String> {
    let [row_payoffs, column_payoffs, row_mix, column_mix] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, row_payoffs, column_payoffs) = game_matrices(row_payoffs, column_payoffs)?;
    let row_mix = distribution(row_mix, rows)?;
    let column_mix = distribution(column_mix, cols)?;
    let row_utilities: Vec<f64> = (0..rows)
        .map(|row| {
            (0..cols)
                .map(|column| column_mix[column] * row_payoffs[row * cols + column])
                .sum()
        })
        .collect();
    let row_best = row_utilities
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if row_utilities
        .iter()
        .enumerate()
        .any(|(strategy, utility)| row_mix[strategy] > 0.0 && *utility < row_best - 1e-12)
    {
        return Ok(Value::Bool(false));
    }
    let column_utilities: Vec<f64> = (0..cols)
        .map(|column| {
            (0..rows)
                .map(|row| row_mix[row] * column_payoffs[row * cols + column])
                .sum()
        })
        .collect();
    let column_best = column_utilities
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    Ok(Value::Bool(!column_utilities.iter().enumerate().any(
        |(strategy, utility)| column_mix[strategy] > 0.0 && *utility < column_best - 1e-12,
    )))
}

fn column_maximizer_set(args: &[Value]) -> Result<Value, String> {
    let [payoffs, column] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let (rows, cols, payoffs) = matrix(payoffs)?;
    if rows == 0
        || cols == 0
        || payoffs.len() != rows * cols
        || payoffs.iter().any(|value| !value.is_finite())
    {
        return Err(
            "E-GAME-001: payoff matrix must be non-empty, rectangular, and finite".to_string(),
        );
    }
    let column = index(
        column,
        cols,
        "E-GAME-003: opponent strategy is outside the finite carrier",
    )?;
    let best = (0..rows)
        .map(|row| payoffs[row * cols + column])
        .fold(f64::NEG_INFINITY, f64::max);
    Ok(Value::Vector(
        (0..rows)
            .filter(|row| (payoffs[row * cols + column] - best).abs() <= 1e-12)
            .map(|row| row as f64)
            .collect(),
    ))
}

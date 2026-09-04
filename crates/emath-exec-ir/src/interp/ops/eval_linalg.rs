//! Collection and linear-algebra op evaluation: selects, vectors, matrices, stencils, solvers, graphs.

use super::*;

pub(super) fn eval_linalg_op(
    op: &EmirOp,
    registers: &[Value],
    name: &'static str,
) -> Result<Value, EvalFault> {
    match *op {
        EmirOp::VectorCreate(ref elements) => {
            let mut vec = Vec::with_capacity(elements.len());
            for &elem in elements {
                vec.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Vector(vec))
        }
        EmirOp::MatrixCreate {
            rows,
            cols,
            ref elements,
        } => {
            let expected = rows.checked_mul(cols).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "matrix size overflow",
            })?;
            if elements.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "matrix element count does not match rows*cols",
                });
            }
            let mut data = Vec::with_capacity(elements.len());
            for &elem in elements {
                data.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Matrix { rows, cols, data })
        }
        EmirOp::VectorIndex { vector, index } => {
            let vec = vector_of(registers, vector, name)?;
            let raw = f64_of(registers, index, name)?;
            emath_rt::vec_index_checked(vec, raw)
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            let (r_count, c_count, data) = matrix_of(registers, matrix, name)?;
            let raw_r = f64_of(registers, row, name)?;
            let raw_c = f64_of(registers, col, name)?;
            emath_rt::tensor_index_checked(&[r_count, c_count], data, &[raw_r, raw_c])
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::VectorAdd(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::Vector(emath_rt::vec_add(v1, v2)))
        }
        EmirOp::VectorSub(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::Vector(emath_rt::vec_sub(v1, v2)))
        }
        EmirOp::VectorScale(left, right) => {
            // Canonical operand order from admission: (vector, scalar).
            // Still accept (scalar, vector) so older EMIR stays evaluable.
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Vector(v), Value::F64(s)) | (Value::F64(s), Value::Vector(v)) => {
                    Ok(Value::Vector(emath_rt::vec_scale(v, *s)))
                }
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::VectorDot(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::F64(emath_rt::vec_dot(v1, v2)))
        }
        EmirOp::VectorNorm(value) => {
            let v = vector_of(registers, value, name)?;
            Ok(Value::F64(emath_rt::vec_norm(v)))
        }
        EmirOp::VectorLength(value) => {
            let v = vector_of(registers, value, name)?;
            Ok(Value::F64(v.len() as f64))
        }
        EmirOp::Stencil1d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let v = vector_of(registers, input, name)?;
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { left, right } => {
                    emath_rt::EdgePolicy::Dirichlet { left, right }
                }
            };
            Ok(Value::Vector(emath_rt::stencil_1d(
                v,
                weights,
                center as i64,
                edge,
            )))
        }
        EmirOp::Stencil2d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let (rows, cols, data) = matrix_of(registers, input, name)?;
            if matches!(edge, EdgePolicy::Dirichlet { .. }) {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "2D Dirichlet boundary is not yet supported; use Clamp, Neumann, or OneSided",
                });
            }
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { .. } => unreachable!("checked above"),
            };
            let nested = rows_of(data, cols);
            let w9: &[f64; 9] =
                weights
                    .as_slice()
                    .try_into()
                    .map_err(|_| EvalFault::Arithmetic {
                        op: name,
                        detail: "2D stencil weights must have length 9",
                    })?;
            let out = emath_rt::stencil_2d(&nested, w9, (center.0 as i64, center.1 as i64), edge);
            Ok(Value::Matrix {
                rows,
                cols,
                data: flatten_rows(&out),
            })
        }
        EmirOp::Stencil3d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let (shape, data) = tensor_of(registers, input, name)?;
            let w27: &[f64; 27] =
                weights
                    .as_slice()
                    .try_into()
                    .map_err(|_| EvalFault::Arithmetic {
                        op: name,
                        detail: "3D stencil weights must have length 27",
                    })?;
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { left, right } => {
                    emath_rt::EdgePolicy::Dirichlet { left, right }
                }
            };
            emath_rt::stencil_3d_slices_checked(
                shape,
                data,
                w27,
                (center.0 as i64, center.1 as i64, center.2 as i64),
                edge,
            )
            .map(|out| Value::Tensor {
                shape: out.shape,
                data: out.data,
            })
            .map_err(|detail| EvalFault::Arithmetic { op: name, detail })
        }
        EmirOp::MatrixAdd(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            require_same_matrix_shape(r1, c1, r2, c2, name)?;
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_add(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixSub(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            require_same_matrix_shape(r1, c1, r2, c2, name)?;
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_sub(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixScale(left, right) => {
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Matrix { rows, cols, data }, Value::F64(s))
                | (Value::F64(s), Value::Matrix { rows, cols, data }) => {
                    let nested = rows_of(data, *cols);
                    Ok(Value::Matrix {
                        rows: *rows,
                        cols: *cols,
                        data: flatten_rows(&emath_rt::mat_scale(&nested, *s)),
                    })
                }
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::MatrixMulVector(matrix, vector) => {
            let (_, cols, m_data) = matrix_of(registers, matrix, name)?;
            let v = vector_of(registers, vector, name)?;
            require_equal_len(v.len(), cols, name, "matrix×vector width mismatch")?;
            let nested = rows_of(m_data, cols);
            Ok(Value::Vector(emath_rt::mat_mul_vec(&nested, v)))
        }
        EmirOp::MatrixMulMatrix(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            if c1 != r2 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "matrix product inner dimensions mismatch",
                });
            }
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_mul_mat(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c2,
                data,
            })
        }
        EmirOp::MatrixTranspose(value) => {
            // Flat row-major involution. Nested `Vec<Vec<f64>>` cannot
            // store a 0-column (or 0-row) extent, and `chunks_exact(0)`
            // panics, so `transpose(transpose(A))` must not go through it.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let mut out = vec![0.0; data.len()];
            if rows > 0 && cols > 0 {
                for r in 0..rows {
                    let src = r * cols;
                    for c in 0..cols {
                        out[c * rows + r] = data[src + c];
                    }
                }
            }
            Ok(Value::Matrix {
                rows: cols,
                cols: rows,
                data: out,
            })
        }
        EmirOp::EigenSymmetric(value) => {
            // Deterministic cyclic Jacobi over the matrix's dense
            // storage; eigenvalues ASCENDING. Typed refusals for
            // non-square / non-symmetric input (E-LINALG-001/002) —
            // never a garbage spectrum.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let (values, _vectors) =
                emath_rt::linalg::jacobi_eigen(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(values))
        }
        EmirOp::EigenVectorsSymmetric(value) => {
            // Column j is the unit eigenvector for eigenvalue j.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let (_values, vectors) =
                emath_rt::linalg::jacobi_eigen(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            let n = vectors.len();
            let mut out = vec![0.0; n * n];
            for (j, column) in vectors.iter().enumerate() {
                for (i, entry) in column.iter().enumerate() {
                    out[i * n + j] = *entry;
                }
            }
            Ok(Value::Matrix {
                rows: n,
                cols: n,
                data: out,
            })
        }
        EmirOp::SvdSingularValues(value) => {
            // Thin rank via the symmetric AᵀA eigenproblem; DESCENDING.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let singular =
                emath_rt::linalg::svd_singular_values(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(singular))
        }
        EmirOp::SvdFactors(value) => {
            // Packed row-major `[U; s; Vᵀ]` (width max(cols, r), zero
            // padding): rows 0..m = U, row m = s, rows m+1..m+1+r = Vᵀ.
            // The kernel returns the packed block directly.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let packed =
                emath_rt::linalg::svd_factors_packed(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            // rank = min(rows, cols) <= cols, so the packing width is
            // cols; invert the row count to recover the rank.
            let width = cols;
            let rank = packed.len() / width - rows - 1;
            Ok(Value::Matrix {
                rows: rows + 1 + rank,
                cols: width,
                data: packed,
            })
        }
        EmirOp::CgSolve(a_value, b_value) => {
            // Conjugate gradient over A's dense storage; SPD convergence
            // is checked, and a non-converging system refuses typed
            // (E-LINALG-003) — never a silently wrong x.
            let (rows, cols, data) = matrix_of(registers, a_value, name)?;
            let b = vector_of(registers, b_value, name)?;
            let x = emath_rt::linalg::cg_solve(&data, rows, cols, &b).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(x))
        }
        EmirOp::LinearSolve(a_value, b_value) => {
            let (rows, cols, data) = matrix_of(registers, a_value, name)?;
            let b = vector_of(registers, b_value, name)?;
            let x = emath_rt::linalg::linear_solve(&data, rows, cols, b).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(x))
        }
        EmirOp::LuFactors(value) => {
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let factors = emath_rt::linalg::lu_factors(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows: 2 * rows + 1,
                cols,
                data: factors,
            })
        }
        EmirOp::QrFactors(value) => {
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let factors = emath_rt::linalg::qr_factors(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows: rows + cols,
                cols,
                data: factors,
            })
        }
        EmirOp::OuterProduct(left_value, right_value) => {
            let left = vector_of(registers, left_value, name)?;
            let right = vector_of(registers, right_value, name)?;
            let data = left
                .iter()
                .flat_map(|left| right.iter().map(move |right| left * right))
                .collect();
            Ok(Value::Matrix {
                rows: left.len(),
                cols: right.len(),
                data,
            })
        }
        EmirOp::GraphReachable(adj, source) => {
            // BFS reachability mask over the dense adjacency carrier;
            // vertices are indices and discovery is ascending-index —
            // deterministic by construction.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let mask =
                emath_rt::graph::reachability(&data, rows, cols, source).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(mask))
        }
        EmirOp::GraphBfsOrder(adj, source) => {
            // BFS visit order: source first, ascending-index discovery
            // (breadth-first, never depth-first, never insertion-order).
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let order = emath_rt::graph::bfs_order(&data, rows, cols, source).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(order))
        }
        EmirOp::GraphDijkstra(adj, source) => {
            // Shortest distances over nonnegative weights; unreachable
            // vertices are +Inf; a negative weight refuses typed
            // E-GRAPH-002 (Dijkstra's precondition) — never a silently
            // wrong distance set.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let distances =
                emath_rt::graph::dijkstra(&data, rows, cols, source).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(distances))
        }
        EmirOp::GraphDegreeOut(adj) => {
            // Out-degree = count of nonzero entries per row; in-degree
            // is the same op over the transposed carrier.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let degrees = emath_rt::graph::degree_out(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(degrees))
        }
        EmirOp::GraphLaplacian(adj) => {
            // L = D − A: the unnormalized Laplacian; the
            // spectrum composes through the EXISTING symmetric eigen
            // op (undirected carriers only — the documented fence).
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let laplacian = emath_rt::graph::laplacian(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows,
                cols,
                data: laplacian,
            })
        }
        EmirOp::GraphSymmetrize(adj) => {
            // S = (A + Aᵀ)/2: the weight-preserving
            // symmetrization; the output composes through the
            // EXISTING laplacian/symmetric-eigen path.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let symmetrized = emath_rt::graph::symmetrize(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows,
                cols,
                data: symmetrized,
            })
        }
        EmirOp::GraphBellmanFord(adj, source) => {
            // Negative-edge shortest paths: relaxation-based;
            // a reachable negative cycle refuses E-GRAPH-005 — never
            // fabricated distances.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let distances =
                emath_rt::graph::bellman_ford(&data, rows, cols, source).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(distances))
        }
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            if bool_of(registers, condition, name)? {
                register(registers, then_value).cloned()
            } else {
                register(registers, else_value).cloned()
            }
        }
        _ => unreachable!("eval_linalg_op routed a non-matching op"),
    }
}

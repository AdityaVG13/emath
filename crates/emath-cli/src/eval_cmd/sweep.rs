//! The `emath sweep` lane: cartesian parameter grids over one admitted
//! function spec.
//!
//! Sweep is `emath eval --function` iterated over a grid with a
//! deterministic evidence artifact. Every cell runs through the SAME
//! generic stack as eval — sema admission, `definition_order` /
//! `lower_definition` EMIR lowering, reference-VM evaluation — and the
//! artifact (`emath.sweep.v1`) carries `meaning_id`, the grid, and
//! per-cell results with no wall-clock field, so byte-identical
//! invocations produce byte-identical output.

use super::*;

/// One parsed `emath sweep` invocation.
pub(crate) struct SweepArgs {
    pub path: PathBuf,
    /// `--function NAME`: the single entrypoint swept (required).
    pub function: String,
    /// Grid axes in command-line order: `(axis name, raw values)`.
    /// The cartesian product enumerates with the first axis slowest.
    pub grid: Vec<(String, Vec<String>)>,
    /// `--expect name=value` rows in command-line order; every cell must
    /// satisfy every expectation.
    pub expects: Vec<(String, String)>,
    /// `--out <file>`: additionally write the JSON artifact to a file.
    pub out: Option<PathBuf>,
    pub json: bool,
}

pub(crate) fn parse_sweep_args(args: &[String]) -> Option<SweepArgs> {
    let mut path = None;
    let mut function = None;
    let mut grid: Vec<(String, Vec<String>)> = Vec::new();
    let mut expects: Vec<(String, String)> = Vec::new();
    let mut out = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--function" => {
                index += 1;
                assign_once(&mut function, args.get(index)?.clone())?;
            }
            "--grid" => {
                index += 1;
                // One `--grid` opens a grid block: every following
                // `name=v1,v2,...` token is an axis until a flag ends
                // the block. The positional file must precede it.
                let mut axes = 0;
                while index < args.len() {
                    let raw = args[index].as_str();
                    if raw.starts_with('-') {
                        break;
                    }
                    let Some(equals) = raw.find('=') else {
                        return None;
                    };
                    let name = raw[..equals].trim().to_string();
                    let values: Vec<String> = raw[equals + 1..]
                        .split(',')
                        .map(|value| value.trim().to_string())
                        .collect();
                    if name.is_empty() || values.iter().any(String::is_empty) {
                        return None;
                    }
                    if grid.iter().any(|(axis, _)| *axis == name) {
                        return None;
                    }
                    grid.push((name, values));
                    axes += 1;
                    index += 1;
                }
                if axes == 0 {
                    return None;
                }
                continue;
            }
            "--expect" => {
                index += 1;
                let raw = args.get(index)?;
                let Some(equals) = raw.find('=') else {
                    return None;
                };
                let name = raw[..equals].trim().to_string();
                let value = raw[equals + 1..].trim().to_string();
                if name.is_empty() || value.is_empty() {
                    return None;
                }
                if expects.iter().any(|(known, _)| *known == name) {
                    return None;
                }
                expects.push((name, value));
            }
            "--out" | "-o" => {
                index += 1;
                assign_once(&mut out, PathBuf::from(args.get(index)?))?;
            }
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    if grid.is_empty() {
        return None;
    }
    Some(SweepArgs {
        path: path?,
        function: function?,
        grid,
        expects,
        out,
        json,
    })
}

/// Cartesian expansion in deterministic order: axes in command-line
/// order, first axis slowest, each axis's values in command-line order.
fn expand_cartesian(axes: &[(String, Vec<String>)]) -> Vec<Vec<(String, String)>> {
    let mut cells: Vec<Vec<(String, String)>> = vec![Vec::new()];
    for (name, values) in axes {
        let mut next = Vec::new();
        for prefix in &cells {
            for value in values {
                let mut cell = prefix.clone();
                cell.push((name.clone(), value.clone()));
                next.push(cell);
            }
        }
        cells = next;
    }
    cells
}

enum CellStatus {
    Ok,
    Mismatch { want: String, got: String },
    Error { code: String, message: String },
}

struct CellRecord {
    /// Rendered bindings in grid-axis order.
    bindings: Vec<(String, String)>,
    /// Rendered outputs in declaration order.
    outputs: Vec<(String, String)>,
    status: CellStatus,
}

impl CellRecord {
    fn human_line(&self, function: &str, expects: &[(String, String)]) -> String {
        let mut line = format!("{function}");
        for (name, value) in &self.bindings {
            line.push_str(&format!(" {name}={value}"));
        }
        line.push(':');
        // The printed values are the computed values of the expected
        // outputs (expect order); with no expectations, every declared
        // output renders. Matches the proximity-prize sweep ledger.
        let gots: Vec<String> = if expects.is_empty() {
            self.outputs
                .iter()
                .map(|(_, value)| value.clone())
                .collect()
        } else {
            expects
                .iter()
                .filter_map(|(name, _)| {
                    self.outputs
                        .iter()
                        .find(|(out_name, _)| out_name == name)
                        .map(|(_, value)| value.clone())
                })
                .collect()
        };
        if !gots.is_empty() {
            line.push(' ');
            line.push_str(&gots.join(" "));
        }
        match &self.status {
            CellStatus::Ok => line.push_str(" OK"),
            CellStatus::Mismatch { want, got: _ } => {
                line.push_str(&format!(" MISMATCH (want {want})"));
            }
            CellStatus::Error { code, message } => {
                line.push_str(&format!(" error {code}: {message}"));
            }
        }
        line
    }

    fn json(&self, index: usize) -> String {
        let mut cell = JsonWriter::object();
        cell.int("index", index as u64);
        cell.object_field("bindings", &value_map_json(&self.bindings));
        cell.object_field("outputs", &value_map_json(&self.outputs));
        match &self.status {
            CellStatus::Ok => {
                cell.string("status", "ok");
            }
            CellStatus::Mismatch { want, got } => {
                cell.string("status", "mismatch");
                cell.string("want", want);
                cell.string("got", got);
            }
            CellStatus::Error { code, message } => {
                cell.string("status", "error");
                cell.string("code", code);
                cell.string("error", message);
            }
        }
        cell.finish()
    }
}

/// The deterministic `emath.sweep.v1` artifact: meaning identity, the
/// grid as declared, and per-cell results. No wall-clock anywhere.
fn render_artifact(
    function: &str,
    meaning_id: &str,
    grid: &[(String, Vec<String>)],
    expects: &[(String, String)],
    cells: &[CellRecord],
) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.sweep.v1");
    object.int("schema_version", 1);
    object.string("function", function);
    object.string("meaning_id", meaning_id);
    let axes: Vec<String> = grid
        .iter()
        .map(|(name, values)| {
            let mut axis = JsonWriter::object();
            axis.string("name", name);
            axis.strings("values", values);
            axis.finish()
        })
        .collect();
    let mut grid_object = JsonWriter::object();
    grid_object.objects("axes", &axes);
    object.object_field("grid", &grid_object.finish());
    object.object_field("expect", &value_map_json(expects));
    let cells_json: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| cell.json(index))
        .collect();
    object.objects("cells", &cells_json);
    let (ok, mismatch, error) = cells.iter().fold((0, 0, 0), |acc, cell| match cell.status {
        CellStatus::Ok => (acc.0 + 1, acc.1, acc.2),
        CellStatus::Mismatch { .. } => (acc.0, acc.1 + 1, acc.2),
        CellStatus::Error { .. } => (acc.0, acc.1, acc.2 + 1),
    });
    let mut summary = JsonWriter::object();
    summary.int("total", cells.len() as u64);
    summary.int("ok", ok);
    summary.int("mismatch", mismatch);
    summary.int("error", error);
    object.object_field("summary", &summary.finish());
    object.finish()
}

/// The sweep lane: admit the source once, select the `--function`
/// entrypoint, validate the grid against the declared inputs, then
/// evaluate every cartesian cell through the reference VM and check the
/// expectations. Failures are typed E-EVAL-* refusals (shared with
/// eval) or per-cell statuses in the artifact; never a silent guess.
pub(crate) fn dispatch_sweep(args: SweepArgs) -> CliExit {
    let source = match std::fs::read_to_string(&args.path) {
        Ok(source) if has_declaration_content(&source) => source,
        Ok(_) => {
            return refuse_eval_coded(
                "E-PKG-081",
                &format!("source has no declarations ({})", args.path.display()),
                args.json,
            );
        }
        Err(_) => {
            return refuse_eval_coded(
                "E-PKG-080",
                &format!("cannot read source file ({})", args.path.display()),
                args.json,
            );
        }
    };
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(&args.path.display().to_string(), &source);
    if result.diagnostics.has_errors() {
        print_diagnostics(&result.diagnostics);
        if args.json {
            print_json_diagnostics(
                "sweep",
                false,
                &json_diagnostics_entries(&result.diagnostics),
            );
        }
        return EXIT_REFUSED;
    }
    let package = result.package;
    let eval_args = EvalArgs {
        path: args.path.clone(),
        world: None,
        json: args.json,
        function: Some(args.function.clone()),
        set: Vec::new(),
    };
    let declaration = match select_entrypoint(&eval_args, &package) {
        Ok(declaration) => declaration,
        Err((code, message)) => return refuse_eval_coded(code, &message, args.json),
    };
    if !declaration.state.is_empty() {
        return refuse_eval_coded(
            "E-EVAL-001",
            &format!(
                "entrypoint `{}` is stateful; `emath sweep` executes only stateless function declarations",
                declaration.name.leaf()
            ),
            args.json,
        );
    }
    // Same input-type support contract as eval (E-EVAL-006).
    for field in &declaration.inputs {
        let supported = match package.ty(field.ty) {
            Some(TypeNode::Float64) => true,
            Some(TypeNode::Int | TypeNode::Nat) => true,
            Some(TypeNode::Vector { element, .. }) => {
                matches!(
                    &**element,
                    TypeNode::Float64 | TypeNode::Int | TypeNode::Nat
                )
            }
            _ => false,
        };
        if !supported {
            return refuse_eval_coded(
                "E-EVAL-006",
                &format!(
                    "input `{}` has a type `emath sweep` cannot bind (Float64, Int, Nat, Vector[Float64], Vector[Int], and Vector[Nat] only)",
                    field.name
                ),
                args.json,
            );
        }
    }
    for (axis, _) in &args.grid {
        if !declaration.inputs.iter().any(|field| field.name == *axis) {
            return refuse_eval_coded(
                "E-EVAL-005",
                &format!(
                    "`--grid` names `{axis}`, which is not a declared input of `{}`",
                    declaration.name.leaf()
                ),
                args.json,
            );
        }
    }
    for (name, _) in &args.expects {
        if !declaration.outputs.iter().any(|field| field.name == *name) {
            return refuse_eval_coded(
                "E-EVAL-005",
                &format!(
                    "`--expect` names `{name}`, which is not a declared output of `{}`",
                    declaration.name.leaf()
                ),
                args.json,
            );
        }
    }
    // Parse every axis value once, against the declared input type
    // (same value grammar as `--set`).
    let mut value_by_raw: std::collections::HashMap<(String, String), Value> =
        std::collections::HashMap::new();
    for (axis, raw_values) in &args.grid {
        let declared = declaration
            .inputs
            .iter()
            .find(|field| field.name == *axis)
            .and_then(|field| package.ty(field.ty).cloned());
        for raw in raw_values {
            let Some(value) = parse_set_value_for(declared.as_ref(), raw) else {
                return refuse_eval_coded(
                    "E-EVAL-005",
                    &format!(
                        "cannot parse `--grid {axis}={raw}` as a value of the declared input type"
                    ),
                    args.json,
                );
            };
            value_by_raw.insert((axis.clone(), raw.clone()), value);
        }
    }
    let missing: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .filter(|name| !args.grid.iter().any(|(axis, _)| axis == name))
        .collect();
    if !missing.is_empty() {
        return refuse_eval_coded(
            "E-EVAL-004",
            &format!(
                "missing grid axis for input(s): {} (the grid must bind every declared input)",
                missing.join(", ")
            ),
            args.json,
        );
    }
    let meaning_id = match package.meaning_id(&[]) {
        Ok(id) => id.to_string(),
        Err(error) => {
            return refuse_eval_coded(
                "E-EVAL-007",
                &format!("meaning identity refused: {error:?}"),
                args.json,
            );
        }
    };
    // One admission, N cells: only the reference-VM evaluation repeats.
    let empty_state = BTreeMap::new();
    let mut cells: Vec<CellRecord> = Vec::new();
    for cell in expand_cartesian(&args.grid) {
        let mut bindings: BTreeMap<String, Value> = BTreeMap::new();
        let mut rendered_bindings: Vec<(String, String)> = Vec::new();
        for (name, raw) in &cell {
            let value = value_by_raw
                .get(&(name.clone(), raw.clone()))
                .expect("grid value was parsed during validation");
            bindings.insert(name.clone(), value.clone());
            rendered_bindings.push((name.clone(), value.to_string()));
        }
        let (outputs, status) =
            match eval_definitions_values(&package, declaration, &bindings, &empty_state) {
                Ok(definitions) => {
                    let outputs: Vec<(String, String)> = declaration
                        .outputs
                        .iter()
                        .filter_map(|field| {
                            definitions
                                .get(&field.name)
                                .map(|value| (field.name.clone(), value.to_string()))
                        })
                        .collect();
                    let mut failures: Vec<(String, String, String)> = Vec::new();
                    for (name, want) in &args.expects {
                        let got = outputs
                            .iter()
                            .find(|(out_name, _)| out_name == name)
                            .map(|(_, value)| value.clone())
                            .unwrap_or_else(|| "<absent>".to_string());
                        if got != *want {
                            failures.push((name.clone(), want.clone(), got));
                        }
                    }
                    let status = if failures.is_empty() {
                        CellStatus::Ok
                    } else {
                        let (_, want, got) = failures.swap_remove(0);
                        CellStatus::Mismatch { want, got }
                    };
                    (outputs, status)
                }
                Err(verdict) => (
                    Vec::new(),
                    CellStatus::Error {
                        code: "E-EVAL-007".to_string(),
                        message: verdict
                            .reason_text()
                            .unwrap_or_else(|| verdict.to_string())
                            .to_string(),
                    },
                ),
            };
        cells.push(CellRecord {
            bindings: rendered_bindings,
            outputs,
            status,
        });
    }
    let all_ok = cells
        .iter()
        .all(|cell| matches!(cell.status, CellStatus::Ok));
    if args.json {
        println!(
            "{}",
            render_artifact(
                declaration.name.leaf(),
                &meaning_id,
                &args.grid,
                &args.expects,
                &cells
            )
        );
    } else {
        for cell in &cells {
            println!(
                "{}",
                cell.human_line(declaration.name.leaf(), &args.expects)
            );
        }
    }
    if let Some(out) = &args.out {
        let artifact = render_artifact(
            declaration.name.leaf(),
            &meaning_id,
            &args.grid,
            &args.expects,
            &cells,
        );
        if std::fs::write(out, format!("{artifact}\n")).is_err() {
            eprintln!("error: cannot write sweep artifact ({})", out.display());
            return CliExit::Usage;
        }
    }
    if all_ok { EXIT_OK } else { EXIT_REFUSED }
}

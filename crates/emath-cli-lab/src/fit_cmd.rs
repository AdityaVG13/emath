//! `emath fit`: the honest refusal. Least-squares fitting is ordinary
//! authored mathematics — `use numerics.levenberg` and call
//! `levenberg_fit` from an `emath function` — not a per-method
//! compiler lane (`language/modules/numerics/levenberg.emath`).

use super::{CliExit, EXIT_REFUSED, json_diagnostic_entry, split_error_code};
use emath_artifact::JsonWriter;

pub(crate) fn dispatch_fit(args: &FitArgs) -> CliExit {
    refuse_fit(
        "E-KIND-GONE: `emath fit` is not a constructor command. `emath model` is not a core kind. Write an ordinary `emath function` and `emath run`.",
        args.json,
    )
}

pub(crate) struct FitArgs {
    json: bool,
}

pub(crate) fn parse_fit_args(args: &[String]) -> Result<FitArgs, String> {
    // The path argument is validated (exactly one positional file) but
    // not stored: the lane refuses before reading any file.
    let mut saw_path = false;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown flag `{other}`"));
            }
            other if saw_path => {
                return Err(format!("unexpected extra argument `{other}`"));
            }
            _ => saw_path = true,
        }
    }
    if !saw_path {
        return Err("missing <file.emath>".to_string());
    }
    Ok(FitArgs { json })
}

fn refuse_fit(text: &str, json: bool) -> CliExit {
    eprintln!("error: {text}");
    if json {
        let (code, message) = split_error_code(text).unwrap_or(("E-FIT-000", text));
        let mut out = JsonWriter::object();
        out.string("command", "fit");
        out.bool("admitted", false);
        out.objects(
            "diagnostics",
            &[json_diagnostic_entry(code, "error", message)],
        );
        print!("{}", out.finish());
    }
    EXIT_REFUSED
}

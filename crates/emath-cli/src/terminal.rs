//! Terminal and environment conventions (`NO_COLOR`, CI, TERM=dumb, TTY detection).
//!
//! Conforms to <https://no-color.org> and standard Unix CLI conventions:
//! - `NO_COLOR`: When set to any value, suppress all ANSI color escape sequences.
//! - TERM=dumb: Terminal lacks capabilities; suppress colors and cursor movements.
//! - Non-TTY: When stdout or stderr is piped/redirected, suppress colors automatically.
//! - CI: Continuous integration environment; force non-interactive behavior.
//! - `--color <auto|always|never>` and `--no-color` flag overrides.

use crate::pedagogy::PedagogicError;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicU8, Ordering};

const COLOR_AUTO: u8 = 0;
const COLOR_ALWAYS: u8 = 1;
const COLOR_NEVER: u8 = 2;

static COLOR_MODE: AtomicU8 = AtomicU8::new(COLOR_AUTO);

/// Set the color mode explicitly.
pub fn set_color_mode(mode: &str) -> Result<(), PedagogicError> {
    match mode {
        "always" => {
            COLOR_MODE.store(COLOR_ALWAYS, Ordering::Relaxed);
            Ok(())
        }
        "never" => {
            COLOR_MODE.store(COLOR_NEVER, Ordering::Relaxed);
            Ok(())
        }
        "auto" => {
            COLOR_MODE.store(COLOR_AUTO, Ordering::Relaxed);
            Ok(())
        }
        other => Err(PedagogicError::new(
            "E-CLI-INVALID-ARGUMENT",
            format!("invalid color mode `{other}`; expected `auto`, `always`, or `never`"),
            format!("value `{other}` for `--color`"),
            "pass `--color auto`, `--color always`, or `--color never`",
        )
        .with_flag("--color")
        .with_usage("emath <command> [--color auto|always|never] [--no-color]")),
    }
}

/// Force suppression of colors (equivalent to `--color never` or `NO_COLOR=1`).
pub fn set_no_color() {
    COLOR_MODE.store(COLOR_NEVER, Ordering::Relaxed);
}

/// Reset color mode back to default auto.
pub fn reset_color_mode() {
    COLOR_MODE.store(COLOR_AUTO, Ordering::Relaxed);
}

/// Current color mode as a string: "auto", "always", or "never".
#[must_use]
pub fn color_mode() -> &'static str {
    match COLOR_MODE.load(Ordering::Relaxed) {
        COLOR_ALWAYS => "always",
        COLOR_NEVER => "never",
        _ => "auto",
    }
}

/// Extracts and applies `--color <mode>`, `--color=<mode>`, and `--no-color` from the argument list.
///
/// Returns the remaining arguments with color flags stripped, or a pedagogic error if invalid.
pub fn extract_color_flags(args: &[String]) -> Result<Vec<String>, PedagogicError> {
    let mut cleaned = Vec::with_capacity(args.len());
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            if let Some(rest) = args.get(index..) {
                cleaned.extend(rest.iter().cloned());
            }
            break;
        } else if arg == "--no-color" {
            set_no_color();
            index += 1;
        } else if let Some(mode) = arg.strip_prefix("--color=") {
            set_color_mode(mode)?;
            index += 1;
        } else if arg == "--color" {
            index += 1;
            let val = args.get(index);
            if val.is_none_or(|v| v.starts_with('-') && v != "-") {
                return Err(PedagogicError::new(
                    "E-CLI-MISSING-VALUE",
                    "flag `--color` requires a value (`auto`, `always`, or `never`)",
                    "flag `--color` (missing value)",
                    "pass `--color auto`, `--color always`, or `--color never`",
                )
                .with_flag("--color")
                .with_usage("emath <command> [--color auto|always|never] [--no-color]"));
            }
            if let Some(val_str) = val {
                set_color_mode(val_str)?;
            }
            index += 1;
        } else {
            cleaned.push(arg.clone());
            index += 1;
        }
    }
    Ok(cleaned)
}

/// Returns true if running in a Continuous Integration environment.
#[must_use]
pub fn is_ci() -> bool {
    std::env::var_os("CI").is_some()
        || std::env::var_os("CONTINUOUS_INTEGRATION").is_some()
        || std::env::var_os("GITHUB_ACTIONS").is_some()
}

/// Returns true if execution is interactive (stdin is a TTY and not in CI).
#[must_use]
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && !is_ci()
}

/// Returns true if stderr should use ANSI color formatting.
#[must_use]
pub fn should_color_stderr() -> bool {
    match COLOR_MODE.load(Ordering::Relaxed) {
        COLOR_ALWAYS => true,
        COLOR_NEVER => false,
        _ => {
            if std::env::var_os("NO_COLOR").is_some() {
                return false;
            }
            if let Ok(term) = std::env::var("TERM") {
                if term == "dumb" {
                    return false;
                }
            }
            std::io::stderr().is_terminal()
        }
    }
}

/// Returns true if stdout should use ANSI color formatting.
#[must_use]
pub fn should_color_stdout() -> bool {
    match COLOR_MODE.load(Ordering::Relaxed) {
        COLOR_ALWAYS => true,
        COLOR_NEVER => false,
        _ => {
            if std::env::var_os("NO_COLOR").is_some() {
                return false;
            }
            if let Ok(term) = std::env::var("TERM") {
                if term == "dumb" {
                    return false;
                }
            }
            std::io::stdout().is_terminal()
        }
    }
}

// Stderr styling functions (used for pedagogic errors and diagnostics)

#[must_use]
pub fn red(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[31m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn bold_red(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[1;31m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn green(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn bold_green(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[1;32m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn yellow(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[33m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn bold_yellow(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[1;33m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn cyan(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[36m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn bold(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[1m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn dim(text: &str) -> String {
    if should_color_stderr() {
        format!("\x1b[2m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

// Stdout styling functions (used for doctor, triage, and human displays)

#[must_use]
pub fn stdout_green(text: &str) -> String {
    if should_color_stdout() {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn stdout_bold_red(text: &str) -> String {
    if should_color_stdout() {
        format!("\x1b[1;31m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn stdout_bold_green(text: &str) -> String {
    if should_color_stdout() {
        format!("\x1b[1;32m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn stdout_yellow(text: &str) -> String {
    if should_color_stdout() {
        format!("\x1b[33m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn stdout_cyan(text: &str) -> String {
    if should_color_stdout() {
        format!("\x1b[36m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn stdout_bold(text: &str) -> String {
    if should_color_stdout() {
        format!("\x1b[1m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[must_use]
pub fn stdout_dim(text: &str) -> String {
    if should_color_stdout() {
        format!("\x1b[2m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

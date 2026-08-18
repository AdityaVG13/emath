//! Command catalog used by help, `--help`, `--version`, and unknown-command
//! hints. Keep this list the single source for first-try discoverability.

/// Every top-level token `emath <command>` accepts (plus version aliases).
pub const COMMANDS: &[&str] = &[
    "check",
    "plan",
    "planner",
    "build",
    "parse",
    "signature",
    "genesis",
    "compile",
    "world",
    "portfolio",
    "import",
    "artifact",
    "architecture",
    "new",
    "fmt",
    "explain",
    "run",
    "test",
    "bench",
    "verify",
    "inspect",
    "diff",
    "doctor",
    "vendor",
    "provider",
    "fork",
    "agent",
    "help",
    "version",
];

/// One-line usage after `emath` for a known command.
#[must_use]
pub fn command_usage(command: &str) -> Option<&'static str> {
    Some(match command {
        "check" => "check <file.emath> [--json]",
        "plan" => "plan <file.emath> [--json]",
        "planner" => "planner <file.emath> [--json] [--parametric]",
        "build" => "build <file.emath> [--out <dir>] [--verify] [--json]",
        "parse" => "parse --forest <file.emath> [--out <dir>]",
        "signature" => "signature <file.emath> [--out <dir>]",
        "genesis" => "genesis <file.emath> --out <dir>",
        "compile" => "compile --parametric <file.emath> --out <dir> [--world LABEL]",
        "world" => "world show WORLD_ID --dir <dir>",
        "portfolio" => "portfolio show PORTFOLIO_ID --dir <dir>",
        "import" => "import modelica <file.mo> [--json]",
        "artifact" => "artifact check|battery <dir>",
        "architecture" => "architecture",
        "new" => "new <name> [--out <dir>]",
        "fmt" => "fmt <file.emath>",
        "explain" => "explain <file.emath> [<symbol>]",
        "run" => "run <file.emath> [--out <dir>]",
        "test" => "test <file.emath> [--out <dir>]",
        "bench" => "bench <file.emath>",
        "verify" => "verify <artifact-dir>",
        "inspect" => "inspect <artifact-dir>",
        "diff" => "diff <a.emath> <b.emath>",
        "doctor" => "doctor",
        "vendor" => "vendor --out <dir>",
        "provider" => "provider list|inspect <id>|test <id>",
        "fork" => "fork status|sync [--dry-run]",
        "agent" => "agent check|plan|build <file.emath> [--out <dir>]",
        "help" => "help [<command>]",
        "version" | "--version" | "-V" => "version",
        "capabilities" => "capabilities [--json]",
        "robot-docs" => "robot-docs [guide]",
        _ => return None,
    })
}

/// Short description printed by `emath help <command>` / `emath <command> --help`.
#[must_use]
pub fn command_summary(command: &str) -> Option<&'static str> {
    Some(match command {
        "check" => "parse + admit, no codegen; `--json` emits codes and admission",
        "plan" => "admit + goals + deterministic native resolution plan",
        "planner" => "provider-registry planning; `--parametric` lifts missing operators",
        "build" => "full pipeline to a published artifact (default out: target/emath)",
        "parse" => "genesis glyphs + bounded parse forest",
        "signature" => "arity/fixity/type-variable signature inference",
        "genesis" => "world interpretation + portfolio + answer receipt",
        "compile" => "parametric generated crate for an admitted world",
        "world" => "print one world candidate artifact",
        "portfolio" => "print one interpretation portfolio artifact",
        "import" => "retain a Modelica subset as foreign-model declarations",
        "artifact" => "independent checker (`check`) or seeded negative-control battery",
        "architecture" => "provider-neutral pipeline map",
        "new" => "deterministic project scaffold; refuses overwrite (E-TLT-011)",
        "fmt" => "canonical-form check (full rewrite is Phase 4)",
        "explain" => "plan-level goal/provider explanation",
        "run" => "build then execute the generated crate (library crates run example tests)",
        "test" => "build with `--verify`; empty test surface is E-TLT-012",
        "bench" => "typed refusal E-TLT-004 until the comparison ruleset lands",
        "verify" => "independent artifact re-verification (same as `artifact check`)",
        "inspect" => "print committed artifact manifests",
        "diff" => "content-id fingerprint comparison of parse-admitted sources",
        "doctor" => "toolchain presence: rustc, cargo, rustfmt, clippy",
        "vendor" => "offline dependency lock snapshot",
        "provider" => "built-in provider descriptors; planned ids stay planned",
        "fork" => "upstream pin status; network sync refused offline (E-TLT-006)",
        "agent" => "structured emath.agent envelope; cannot bypass admission/plan/checks",
        "help" => "this catalog; `emath help <command>` prints one command",
        "version" | "--version" | "-V" => "print the emath-cli crate version",
        "capabilities" => "machine contract: commands, flags, exit codes, env vars",
        "robot-docs" => "paste-ready agent handbook (`guide`)",
        _ => return None,
    })
}

/// Closest known command for a typo, if the edit distance is small.
#[must_use]
pub fn suggest_command(unknown: &str) -> Option<&'static str> {
    let needle = unknown.trim_start_matches('-');
    if needle.is_empty() {
        return None;
    }
    let mut best: Option<(&'static str, usize)> = None;
    for command in COMMANDS {
        if *command == needle {
            return Some(command);
        }
        let distance = edit_distance(needle, command);
        let prefix = command.starts_with(needle) || needle.starts_with(command);
        let score = if prefix {
            distance.saturating_sub(1)
        } else {
            distance
        };
        if score <= 2 && best.is_none_or(|(_, current)| score < current) {
            best = Some((command, score));
        }
    }
    best.map(|(command, _)| command)
}

/// Deterministic `name version` line (no git SHA, no timestamp).
#[must_use]
pub fn version_text() -> String {
    format!("emath {}", env!("CARGO_PKG_VERSION"))
}

#[must_use]
pub fn wants_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

#[must_use]
pub fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

/// Usage + one-line summary for a single command. Returns `None` if unknown.
#[must_use]
pub fn command_help_text(command: &str) -> Option<String> {
    let usage = command_usage(command)?;
    let summary = command_summary(command)?;
    Some(format!(
        "emath {usage}\n{summary}\n\nexit codes: 0 ok, 1 refused/admission diagnostics, 2 usage or io error\nrun `emath help` for the full command list\n"
    ))
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (i, left_ch) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_ch) in right.iter().enumerate() {
            let cost = usize::from(left_ch != right_ch);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typo_chek_suggests_check() {
        assert_eq!(suggest_command("chek"), Some("check"));
    }

    #[test]
    fn typo_buld_suggests_build() {
        assert_eq!(suggest_command("buld"), Some("build"));
    }

    #[test]
    fn typo_verson_suggests_version() {
        assert_eq!(suggest_command("verson"), Some("version"));
    }

    #[test]
    fn nonsense_has_no_suggestion() {
        assert_eq!(suggest_command("zzzzzzzz"), None);
    }

    #[test]
    fn every_catalogued_command_has_usage_and_summary() {
        for command in COMMANDS {
            assert!(command_usage(command).is_some(), "{command} usage");
            assert!(command_summary(command).is_some(), "{command} summary");
            assert!(command_help_text(command).is_some(), "{command} help");
        }
    }
}

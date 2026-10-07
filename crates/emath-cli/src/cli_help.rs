//! Generated help text.

use super::catalog;

/// `help` output. Generated from the command catalog so usage and summary
/// cannot drift from `emath help <command>` / `emath capabilities --json`.
pub fn help_text() -> String {
    let mut out = String::from("emath compiler (constructor layer). Usage:\n");
    for command in catalog::COMMANDS {
        let Some(usage) = catalog::command_usage(command) else {
            continue;
        };
        let Some(summary) = catalog::command_summary(command) else {
            continue;
        };
        out.push_str("  emath ");
        out.push_str(usage);
        out.push('\n');
        out.push_str("      ");
        out.push_str(summary);
        out.push('\n');
        let aliases = catalog::command_aliases(command);
        if !aliases.is_empty() {
            out.push_str("      aliases: ");
            out.push_str(&aliases.join(", "));
            out.push('\n');
        }
    }
    out.push_str("\nexit codes: 0 completed, 2 admission, 3 unmet/partial, 4 fault, 5 checkpoint\n");
    out
}

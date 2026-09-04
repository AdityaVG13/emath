//! The `emath parse` / `signature` frontend commands.

use super::*;

/// `parse <file> [--out <dir>]`: glyphs + bounded parse forest.
pub fn parse_cmd(path: &Path, out: Option<&PathBuf>, forest_only: bool) -> CliExit {
    let analysis = match analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    if forest_only {
        println!(
            "world {}; body {}\nparse_id {}; ambiguity {}; nodes {}; holes {}",
            analysis.file.world_name,
            analysis.file.body_text,
            analysis.parse_id,
            parse_count(&analysis.parse_forest_json, "ambiguity_count"),
            parse_count(&analysis.parse_forest_json, "node_count"),
            analysis.file.explore.len()
        );
    } else {
        println!(
            "world {}; body: {}; explore: {}; protect: {}; answer: {}",
            analysis.file.world_name,
            analysis.file.body_text,
            analysis.file.explore.join(","),
            analysis.file.protect.join(","),
            analysis.file.answer
        );
    }
    if let Err(error) = write_if_requested(out, "parse-forest.json", &analysis.parse_forest_json) {
        eprintln!("error: {error}");
        return EXIT_USAGE;
    }
    EXIT_OK
}

/// Best-effort numeric field reader for CLI summaries over single-line JSON.
pub(super) fn parse_count(json: &str, field: &str) -> String {
    let needle = format!("\"{field}\":");
    json.find(&needle).map_or_else(
        || "?".to_string(),
        |start| {
            let rest = &json[start + needle.len()..];
            let digits = rest
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            if digits.is_empty() {
                "?".to_string()
            } else {
                digits
            }
        },
    )
}

/// `signature <file> [--out <dir>]`: signature + fixity + type variables.
pub fn signature_cmd(path: &Path, out: Option<&PathBuf>) -> CliExit {
    let analysis = match analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let mut arities = String::new();
    for (symbol, arity) in analysis.inference.signature.iter() {
        let _ = writeln!(arities, "  {}: {arity}", symbol.0);
    }
    println!(
        "signature_id {}; world {}\nsymbols:\n{}variables: {:?}",
        analysis.signature_id, analysis.file.world_name, arities, analysis.inference.variables
    );
    if let Err(error) = write_if_requested(out, "signature.json", &analysis.signature_json) {
        eprintln!("error: {error}");
        return EXIT_USAGE;
    }
    EXIT_OK
}

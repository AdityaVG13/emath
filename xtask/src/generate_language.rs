use std::fs;
use std::path::{Path, PathBuf};

use emath_schema::rewrite_declared_semantic_hashes;

pub fn run(export_to_target: bool) -> u8 {
    let source_root = Path::new("language");
    let root = if export_to_target {
        std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("target"))
            .join("language-export")
    } else {
        source_root.to_path_buf()
    };
    if let Err(error) = write_spec_semantic_hashes(&source_root.join("spec")) {
        eprintln!("language image generation refused: {error}");
        return 1;
    }
    match emath_exec_ir::language_image::compile_language_directory(source_root).and_then(
        |distribution| {
            emath_exec_ir::language_image::write_language_distribution(&root, &distribution)?;
            println!(
                "language image: {} capsules={} semantic={} distribution={}",
                root.display(),
                distribution.capsules.len(),
                distribution.image.semantic_hash,
                distribution.image.distribution_hash
            );
            Ok(())
        },
    ) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("language image generation refused: {error:?}");
            1
        }
    }
}

fn write_spec_semantic_hashes(spec: &Path) -> Result<(), String> {
    let mut paths = Vec::new();
    collect_emath_paths(spec, &mut paths)?;
    paths.sort();
    let mut wrote = 0usize;
    let mut matched = 0usize;
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|error| {
            format!("{}: {error}", path.display())
        })?;
        let (rewritten, reports) = rewrite_declared_semantic_hashes(&text).map_err(|issue| {
            format!(
                "{}:{}: {}: {}",
                path.display(),
                issue.line,
                issue.code,
                issue.detail
            )
        })?;
        for report in &reports {
            if report.wrote {
                eprintln!(
                    "semantic_hash {}: {} -> {}",
                    report.feature_id, report.previous, report.computed
                );
                wrote += 1;
            } else {
                matched += 1;
            }
        }
        if rewritten != text {
            fs::write(&path, rewritten).map_err(|error| {
                format!("{}: {error}", path.display())
            })?;
        }
    }
    println!(
        "semantic_hash write-back: wrote {wrote}, already-matched {matched} (reference_body not rewritten)"
    );
    Ok(())
}

fn collect_emath_paths(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|error| format!("{}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_emath_paths(&path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("emath") {
            output.push(path);
        }
    }
    Ok(())
}

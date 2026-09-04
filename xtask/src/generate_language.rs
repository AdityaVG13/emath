use std::path::{Path, PathBuf};

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

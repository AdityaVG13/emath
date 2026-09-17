#![forbid(unsafe_code)]

#[path = "../../src/generate_language.rs"]
mod generate_language;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("usage: cargo xtask <generate-language|demo|build-web|check-wasm|serve-web>");
        std::process::exit(2);
    };
    if command == "generate-language" {
        std::process::exit(i32::from(generate_language::run(
            args.next().as_deref() == Some("--cargo-target"),
        )));
    }

    let status = std::process::Command::new("cargo")
        .args(["run", "-q", "-p", "xtask", "--bin", "xtask", "--features", "demos", "--"])
        .arg(command)
        .args(args)
        .status()
        .expect("failed to launch the full xtask binary");
    std::process::exit(status.code().unwrap_or(1));
}

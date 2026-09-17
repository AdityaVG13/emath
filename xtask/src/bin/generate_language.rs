#![forbid(unsafe_code)]

#[path = "../generate_language.rs"]
mod generate_language;

fn main() {
    let export_to_target = std::env::args().nth(1).as_deref() == Some("--cargo-target");
    std::process::exit(i32::from(generate_language::run(export_to_target)));
}

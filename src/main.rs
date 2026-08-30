use emulsify::cli;

fn main() -> std::process::ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    cli::run(argv)
}

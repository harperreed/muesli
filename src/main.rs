// ABOUTME: CLI entrypoint for muesli command
// ABOUTME: Handles error exit codes and command dispatch

use muesli::Result;

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    println!("Hello, world!");
    Ok(())
}

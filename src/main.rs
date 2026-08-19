//! MIT License
//!
//! Copyright (c) 2026 CoreEmpiric

fn main() {
    if let Err(err) = cargo_kconfig::cli::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

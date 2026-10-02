//! MIT License
//!
//! Copyright (c) 2026 CoreEmpiric
//!
//! Binary entry. Installs [`Telemetry`](cargo_kconfig::Telemetry) then runs
//! [`cargo_kconfig::cli::Cli::run`].
//!
//! # Panics
//!
//! Panics only if telemetry cannot be installed. Every other failure is
//! logged with [`telemetry_error!`](cargo_kconfig::telemetry_error) and
//! `process::exit(1)`.

use cargo_kconfig::cli::Cli;
use cargo_kconfig::{Telemetry, telemetry_error, telemetry_info};

fn main() {
    match Telemetry::new("cargo-kconfig") {
        Ok(_telemetry) => {
            telemetry_info!("telemetry initialised");
            match Cli::run() {
                Ok(()) => {
                    telemetry_info!("Application finished");
                }
                Err(err) => {
                    telemetry_error!("{}", err);
                    std::process::exit(1);
                }
            }
        }
        Err(err) => {
            panic!("cargo-kconfig: telemetry init failed: {err}");
        }
    }
}

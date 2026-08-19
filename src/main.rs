//! MIT License
//!
//! Copyright (c) 2026 CoreEmpiric

use cargo_kconfig::{Telemetry, telemetry_error, telemetry_info};

fn main() {
    match Telemetry::new() {
        Ok(_telemetry) => {
            telemetry_info!("Telemetry initialised");
            match cargo_kconfig::cli::run() {
                Ok(()) => {
                    telemetry_info!("main: Application finished");
                }
                Err(err) => {
                    telemetry_error!("{err}");
                    std::process::exit(1);
                }
            }
        }
        Err(err) => {
            panic!("cargo-kconfig: telemetry init failed: {err}");
        }
    }
}

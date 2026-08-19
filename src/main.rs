//! MIT License
//!
//! Copyright (c) 2026 CoreEmpiric
//!
//! Binary entry. The rest of the program is [`cargo_kconfig::Application`].

use cargo_kconfig::{Application, Telemetry, telemetry_error, telemetry_info};

fn main() {
    match Telemetry::new() {
        Ok(_telemetry) => {
            telemetry_info!("Telemetry initialised");
            match Application::run() {
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

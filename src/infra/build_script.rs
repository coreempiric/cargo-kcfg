//! Cargo `build.rs` entry — the same locator → pipeline → writer path as the CLI.
//!
//! Crate build scripts call [`BuildScript::run`] (or the [`run_build_script`] alias).

use crate::error::Error;
use crate::infra::locator::ProjectLocator;
use crate::infra::pipeline::{ArtifactWriter, Pipeline};
use crate::telemetry::telemetry::Telemetry;
use crate::{telemetry_error, telemetry_info};
use std::path::PathBuf;

/// Runs kconfig generation from a crate `build.rs`.
pub struct BuildScript {
    pipeline: Pipeline,
    writer: ArtifactWriter,
}

impl BuildScript {
    pub fn new() -> Self {
        Self {
            pipeline: Pipeline::new(),
            writer: ArtifactWriter::new(),
        }
    }

    /// Discover files, evaluate, write `OUT_DIR` artefacts, and emit `cargo:` lines.
    pub fn run(&self) -> Result<(), Error> {
        let _telemetry = Telemetry::new().map_err(|err| {
            Error::Usage(format!(
                "telemetry init failed: {err}. The kconfig build script cannot continue"
            ))
        })?;
        match self.run_inner() {
            Ok(()) => Ok(()),
            Err(err) => {
                telemetry_error!("{err}");
                Err(err)
            }
        }
    }

    fn run_inner(&self) -> Result<(), Error> {
        let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
            Error::Usage(
                "CARGO_MANIFEST_DIR is not set. run_build_script is for Cargo build.rs".into(),
            )
        })?);
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").map_err(|_| {
            Error::Usage("OUT_DIR is not set. run_build_script is for Cargo build.rs".into())
        })?);

        telemetry_info!(root = %root.display(), "kconfig build.rs starting in {}", root.display());

        let locator = ProjectLocator::new(root);
        let request = locator.resolve(None, None)?;
        let result = self.pipeline.run(&request)?;

        let config_rs = out_dir.join("config.rs");
        let dotconfig_path = out_dir.join(".config");
        self.writer.write(&result, &config_rs, &dotconfig_path)?;
        result.generated.print_cargo_cfg_lines();

        println!("cargo:rerun-if-env-changed=KCONFIG_DEFCONFIG");
        println!("cargo:rerun-if-changed={}", request.kconfig.display());
        println!("cargo:rerun-if-changed={}", request.defconfig.display());
        for loaded in &result.loaded_files {
            println!("cargo:rerun-if-changed={}", loaded.display());
        }

        telemetry_info!(
            symbols = result.evaluated.table.len(),
            config_rs = %config_rs.display(),
            "kconfig build.rs wrote {} and {} ({} symbols)",
            config_rs.display(),
            dotconfig_path.display(),
            result.evaluated.table.len()
        );
        Ok(())
    }
}

impl Default for BuildScript {
    fn default() -> Self {
        Self::new()
    }
}

/// Load, evaluate, and emit artefacts from a crate `build.rs`.
///
/// Discovers `Kconfig` and a unique `*_defconfig` (or `KCONFIG_DEFCONFIG`),
/// writes `config.rs` / `.config` into `OUT_DIR`, and prints `cargo:rustc-cfg`
/// lines. Errors are reported through telemetry and returned.
pub fn run_build_script() -> Result<(), Error> {
    BuildScript::new().run()
}

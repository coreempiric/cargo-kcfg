//! Cargo `build.rs` entry — the same locator → pipeline → writer path as the CLI.
//!
//! Optional. Prefer `include_config!` so member crates do not need a `build.rs`.
//! Use this helper only when the crate itself needs `cargo:rustc-cfg` for
//! `#[cfg(CONFIG_*)]`.

use crate::error::Error;
use crate::infra::locator::ProjectLocator;
use crate::infra::pipeline::{ArtifactWriter, Pipeline};
use crate::telemetry::telemetry::Telemetry;
use crate::{telemetry_error, telemetry_info};
use std::path::PathBuf;

/// Runs kconfig generation from a crate `build.rs`.
///
/// Prefer `cargo_kconfig_macros::include_config!` when the crate only needs
/// `CONFIG_*` constants. Use this type when the crate itself must emit
/// `cargo:rustc-cfg` for `#[cfg(CONFIG_*)]`.
pub struct BuildScript {
    pipeline: Pipeline,
    writer: ArtifactWriter,
}

impl BuildScript {
    /// Create a build-script runner with default pipeline and writer.
    pub fn new() -> Self {
        Self {
            pipeline: Pipeline::new(),
            writer: ArtifactWriter::new(),
        }
    }

    /// Discover files, evaluate, write `OUT_DIR` artefacts, and emit `cargo:` lines.
    ///
    /// Installs [`Telemetry`] with the prefix `cargo-kconfig`. Failures are
    /// logged with [`telemetry_error!`] and then returned.
    ///
    /// # Errors
    ///
    /// - [`Error::Usage`] if `CARGO_MANIFEST_DIR` / `OUT_DIR` is unset, telemetry
    ///   cannot be installed, or files cannot be discovered.
    /// - [`Error::Validation`] / [`Error::Parse`] / [`Error::Io`] from the pipeline.
    pub fn run(&self) -> Result<(), Error> {
        let _telemetry = Telemetry::new("cargo-kconfig").map_err(|err| {
            Error::Usage(format!(
                "telemetry init failed: {err}. The kconfig build script cannot continue"
            ))
        })?;
        match self.run_inner() {
            Ok(()) => Ok(()),
            Err(err) => {
                telemetry_error!("{}", err);
                Err(err)
            }
        }
    }

    fn run_inner(&self) -> Result<(), Error> {
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
            Error::Usage(
                "CARGO_MANIFEST_DIR is not set. run_build_script is for Cargo build.rs".into(),
            )
        })?);
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").map_err(|_| {
            Error::Usage("OUT_DIR is not set. run_build_script is for Cargo build.rs".into())
        })?);

        telemetry_info!(
            manifest = %manifest.display(),
            "kconfig build.rs starting in {}",
            manifest.display()
        );

        let locator = ProjectLocator::discover(manifest.clone())?;
        if locator.root() != manifest.as_path() {
            telemetry_info!(
                workspace = %locator.root().display(),
                "using workspace Kconfig in {}",
                locator.root().display()
            );
        }
        let request = locator.resolve(None, None)?;
        let result = self.pipeline.run(&request)?;

        let config_rs = out_dir.join("config.rs");
        let dotconfig_path = out_dir.join(".config");
        self.writer.write(&result, &config_rs, &dotconfig_path)?;
        result.generated.print_cargo_cfg_lines();

        println!("cargo:rerun-if-env-changed=KCONFIG_DEFCONFIG");
        println!("cargo:rerun-if-env-changed=CARGO_WORKSPACE_DIR");
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
/// walking up from `CARGO_MANIFEST_DIR` to a workspace root when this package
/// has no `Kconfig`. Writes `config.rs` / `.config` into `OUT_DIR`, and prints
/// `cargo:rustc-cfg` lines. Errors are reported through telemetry and returned.
///
/// # Errors
///
/// Same as [`BuildScript::run`].
pub fn run_build_script() -> Result<(), Error> {
    BuildScript::new().run()
}

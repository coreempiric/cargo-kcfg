//! Command objects for `cargo kconfig check|build|test`.
//!
//! [`Cli::run`] parses argv (stripping the extra `kconfig` token Cargo inserts)
//! and dispatches to [`CheckCommand`], [`BuildCommand`], or [`TestCommand`].

use crate::error::Error;
use crate::infra::locator::ProjectLocator;
use crate::infra::pipeline::{ArtifactWriter, GenerateRequest, Pipeline};
use crate::telemetry_info;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

/// Cargo subcommand that evaluates Kconfig definitions and generates Rust constants.
#[derive(Debug, Parser)]
#[command(name = "cargo-kconfig", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Parse, evaluate, and validate without writing artefacts.
    Check(IoArgs),
    /// Validate and write `.config`, `config.rs`, and optional `rustc-cfg` lines.
    Build(BuildArgs),
    /// Validate, generate artefacts, and type-check the generated constants.
    Test(BuildArgs),
}

#[derive(Debug, Clone, clap::Args)]
pub struct IoArgs {
    /// Project root used to resolve `source` paths.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Root Kconfig file.
    #[arg(long)]
    pub kconfig: Option<PathBuf>,
    /// User assignment file (`*_defconfig` or `.config`).
    #[arg(long)]
    pub defconfig: Option<PathBuf>,
}

#[derive(Debug, Clone, clap::Args)]
pub struct BuildArgs {
    #[command(flatten)]
    pub io: IoArgs,
    /// Directory for generated files.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,
    /// Path of the generated `config.rs`.
    #[arg(long)]
    pub config_rs: Option<PathBuf>,
    /// Path of the generated `.config`.
    #[arg(long)]
    pub dotconfig: Option<PathBuf>,
    /// Print `cargo:rustc-cfg=...` lines for enabled bool/tristate options.
    #[arg(long)]
    pub emit_rustc_cfg: bool,
}

impl Cli {
    /// Entry used by [`crate::Application`]. Strips the extra `kconfig` argument Cargo inserts.
    pub fn run() -> Result<(), Error> {
        Self::run_from_args(std::env::args_os())
    }

    pub fn run_from_args<I, T>(args: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let mut args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
        if args.get(1).and_then(|a| a.to_str()) == Some("kconfig") {
            args.remove(1);
        }
        Self::parse_from(args).execute()
    }

    pub fn execute(self) -> Result<(), Error> {
        self.command.execute()
    }
}

impl Commands {
    pub fn execute(self) -> Result<(), Error> {
        match self {
            Self::Check(args) => CheckCommand::new(args).execute(),
            Self::Build(args) => BuildCommand::new(args).execute(),
            Self::Test(args) => TestCommand::new(args).execute(),
        }
    }
}

impl IoArgs {
    fn project_root(&self) -> Result<PathBuf, Error> {
        match &self.root {
            Some(path) => Ok(path.clone()),
            None => std::env::current_dir().map_err(|e| {
                Error::Usage(format!(
                    "cannot determine the current directory: {e}. Pass --root DIR"
                ))
            }),
        }
    }

    fn request(&self) -> Result<GenerateRequest, Error> {
        ProjectLocator::new(self.project_root()?)
            .resolve(self.kconfig.as_deref(), self.defconfig.as_deref())
    }
}

/// `cargo kconfig check` — validate without writing artefacts.
pub struct CheckCommand {
    args: IoArgs,
}

impl CheckCommand {
    pub fn new(args: IoArgs) -> Self {
        Self { args }
    }

    pub fn execute(&self) -> Result<(), Error> {
        let request = self.args.request()?;
        let result = Pipeline::new().run(&request)?;
        telemetry_info!(
            symbols = result.evaluated.table.len(),
            kconfig = %request.kconfig.display(),
            "kconfig: ok ({} symbols from {})",
            result.evaluated.table.len(),
            request.kconfig.display()
        );
        Ok(())
    }
}

/// `cargo kconfig build` — validate and write `.config` / `config.rs`.
pub struct BuildCommand {
    args: BuildArgs,
    typecheck: bool,
}

impl BuildCommand {
    pub fn new(args: BuildArgs) -> Self {
        Self {
            args,
            typecheck: false,
        }
    }

    fn with_typecheck(mut self) -> Self {
        self.typecheck = true;
        self
    }

    pub fn execute(&self) -> Result<(), Error> {
        let request = self.args.io.request()?;
        let result = Pipeline::new().run(&request)?;
        let out_dir = self
            .args
            .out_dir
            .clone()
            .unwrap_or_else(|| request.root_dir.clone());
        let config_rs = self
            .args
            .config_rs
            .clone()
            .unwrap_or_else(|| out_dir.join("config.rs"));
        let dotconfig = self
            .args
            .dotconfig
            .clone()
            .unwrap_or_else(|| out_dir.join(".config"));
        ArtifactWriter::new().write(&result, &config_rs, &dotconfig)?;
        telemetry_info!(
            symbols = result.evaluated.table.len(),
            config_rs = %config_rs.display(),
            dotconfig = %dotconfig.display(),
            "kconfig: wrote {} and {} ({} symbols)",
            config_rs.display(),
            dotconfig.display(),
            result.evaluated.table.len()
        );
        if self.args.emit_rustc_cfg {
            result.generated.print_cargo_cfg_lines();
        }
        if self.typecheck {
            GeneratedSourceChecker::new().typecheck(&result.generated.config_rs)?;
            telemetry_info!("kconfig: generated constants type-checked");
        }
        Ok(())
    }
}

/// `cargo kconfig test` — build artefacts and type-check the generated constants.
pub struct TestCommand {
    inner: BuildCommand,
}

impl TestCommand {
    pub fn new(args: BuildArgs) -> Self {
        Self {
            inner: BuildCommand::new(args).with_typecheck(),
        }
    }

    pub fn execute(&self) -> Result<(), Error> {
        self.inner.execute()
    }
}

/// Compiles generated `config.rs` with `rustc` so `cargo kconfig test` catches type errors.
pub struct GeneratedSourceChecker;

impl GeneratedSourceChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn typecheck(&self, config_rs: &str) -> Result<(), Error> {
        let dir = self.temp_dir()?;
        let rs_path = dir.join("kconfig_check.rs");
        let mut source = config_rs.to_string();
        source.push_str("\nfn main() {\n    let _ = std::mem::size_of_val(&Tristate::No);\n}\n");
        std::fs::write(&rs_path, source).map_err(|e| Error::io(&rs_path, e))?;
        let output = Command::new("rustc")
            .arg("--edition")
            .arg("2024")
            .arg("--crate-type")
            .arg("bin")
            .arg("-o")
            .arg(dir.join("kconfig_check"))
            .arg(&rs_path)
            .output()
            .map_err(|e| {
                Error::Usage(format!(
                    "failed to invoke rustc: {e}. Install rustc or put it on PATH"
                ))
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim();
            return Err(Error::Usage(if detail.is_empty() {
                "generated config.rs did not compile. Inspect the generated file and fix the Kconfig types".into()
            } else {
                format!("generated config.rs did not compile. rustc reported:\n{detail}")
            }));
        }
        Ok(())
    }

    fn temp_dir(&self) -> Result<PathBuf, Error> {
        let dir = std::env::temp_dir().join(format!("cargo-kconfig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        Ok(dir)
    }
}

impl Default for GeneratedSourceChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Entry used by the binary. Strips the extra `kconfig` argument Cargo inserts.
pub fn run_from_args<I, T>(args: I) -> Result<(), Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Cli::run_from_args(args)
}

pub fn run() -> Result<(), Error> {
    Cli::run()
}

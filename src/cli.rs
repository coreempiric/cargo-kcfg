use crate::error::Error;
use crate::infra::discover::{resolve_defconfig, resolve_kconfig};
use crate::infra::pipeline::{GenerateRequest, run as run_pipeline, write_outputs};
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

/// Entry used by the binary. Strips the extra `kconfig` argument Cargo inserts.
pub fn run_from_args<I, T>(args: I) -> Result<(), Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
    if args.get(1).and_then(|a| a.to_str()) == Some("kconfig") {
        args.remove(1);
    }
    let cli = Cli::parse_from(args);
    dispatch(cli)
}

pub fn run() -> Result<(), Error> {
    run_from_args(std::env::args_os())
}

fn dispatch(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Commands::Check(args) => cmd_check(&args),
        Commands::Build(args) => cmd_build(&args, false),
        Commands::Test(args) => cmd_build(&args, true),
    }
}

fn cmd_check(args: &IoArgs) -> Result<(), Error> {
    let request = resolve_request(args)?;
    let result = run_pipeline(&request)?;
    telemetry_info!(
        symbols = result.evaluated.table.len(),
        kconfig = %request.kconfig.display(),
        "kconfig: ok ({} symbols from {})",
        result.evaluated.table.len(),
        request.kconfig.display()
    );
    Ok(())
}

fn cmd_build(args: &BuildArgs, typecheck: bool) -> Result<(), Error> {
    let request = resolve_request(&args.io)?;
    let result = run_pipeline(&request)?;
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| request.root_dir.clone());
    let config_rs = args
        .config_rs
        .clone()
        .unwrap_or_else(|| out_dir.join("config.rs"));
    let dotconfig = args
        .dotconfig
        .clone()
        .unwrap_or_else(|| out_dir.join(".config"));
    write_outputs(&result, &config_rs, &dotconfig)?;
    telemetry_info!(
        symbols = result.evaluated.table.len(),
        config_rs = %config_rs.display(),
        dotconfig = %dotconfig.display(),
        "kconfig: wrote {} and {} ({} symbols)",
        config_rs.display(),
        dotconfig.display(),
        result.evaluated.table.len()
    );
    if args.emit_rustc_cfg {
        for cfg in &result.generated.rustc_check_cfgs {
            println!("cargo:rustc-check-cfg={cfg}");
        }
        for cfg in &result.generated.rustc_cfgs {
            println!("cargo:rustc-cfg={cfg}");
        }
    }
    if typecheck {
        typecheck_config_rs(&result.generated.config_rs)?;
        telemetry_info!("kconfig: generated constants type-checked");
    }
    Ok(())
}

fn resolve_request(args: &IoArgs) -> Result<GenerateRequest, Error> {
    let root = match &args.root {
        Some(path) => path.clone(),
        None => std::env::current_dir().map_err(|e| {
            Error::Usage(format!(
                "cannot determine the current directory: {e}. Pass --root DIR"
            ))
        })?,
    };
    let kconfig = resolve_kconfig(&root, args.kconfig.as_deref())?;
    let defconfig = resolve_defconfig(&root, args.defconfig.as_deref())?;
    Ok(GenerateRequest::new(root, kconfig, defconfig))
}

fn typecheck_config_rs(config_rs: &str) -> Result<(), Error> {
    let dir = tempfile_or_local()?;
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

fn tempfile_or_local() -> Result<PathBuf, Error> {
    let dir = std::env::temp_dir().join(format!("cargo-kconfig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    Ok(dir)
}

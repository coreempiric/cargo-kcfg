use crate::domain::{EvaluatedConfig, Limits, evaluate};
use crate::error::Error;
use crate::infra::codegen::{Generated, generate};
use crate::infra::defconfig::load_defconfig;
use crate::infra::discover::{resolve_defconfig, resolve_kconfig};
use crate::infra::kconfig::{LoadedKconfig, load_kconfig};
use crate::telemetry::telemetry::Telemetry;
use crate::{telemetry_error, telemetry_info};
use std::path::{Path, PathBuf};

/// Inputs for a full load → evaluate → generate run.
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub root_dir: PathBuf,
    pub kconfig: PathBuf,
    pub defconfig: PathBuf,
    pub limits: Limits,
}

/// Successful pipeline output.
#[derive(Debug, Clone)]
pub struct GenerateResult {
    pub evaluated: EvaluatedConfig,
    pub loaded_files: Vec<PathBuf>,
    pub generated: Generated,
}

impl GenerateRequest {
    pub fn new(root_dir: PathBuf, kconfig: PathBuf, defconfig: PathBuf) -> Self {
        Self {
            root_dir,
            kconfig,
            defconfig,
            limits: Limits::default(),
        }
    }
}

/// Parse definitions and assignments, evaluate, and generate artefacts.
pub fn run(request: &GenerateRequest) -> Result<GenerateResult, Error> {
    let LoadedKconfig {
        table,
        loaded_files,
    } = load_kconfig(&request.root_dir, &request.kconfig, request.limits)?;
    let assignments = load_defconfig(&request.defconfig, request.limits)?;
    let evaluated = evaluate(table, &assignments, request.limits).map_err(Error::Validation)?;
    let generated = generate(&evaluated);
    Ok(GenerateResult {
        evaluated,
        loaded_files,
        generated,
    })
}

pub fn write_outputs(
    result: &GenerateResult,
    config_rs: &Path,
    dotconfig: &Path,
) -> Result<(), Error> {
    if let Some(parent) = config_rs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    if let Some(parent) = dotconfig.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::write(config_rs, &result.generated.config_rs).map_err(|e| Error::io(config_rs, e))?;
    std::fs::write(dotconfig, &result.generated.dotconfig).map_err(|e| Error::io(dotconfig, e))?;
    Ok(())
}

/// Load, evaluate, and emit artefacts from a crate `build.rs`.
///
/// Discovers `Kconfig` and a unique `*_defconfig` (or `KCONFIG_DEFCONFIG`),
/// writes `config.rs` / `.config` into `OUT_DIR`, and prints `cargo:rustc-cfg`
/// lines. Errors are reported through telemetry and returned.
pub fn run_build_script() -> Result<(), Error> {
    let _telemetry = Telemetry::new().map_err(|err| {
        Error::Usage(format!(
            "telemetry init failed: {err}. The kconfig build script cannot continue"
        ))
    })?;
    match run_build_script_inner() {
        Ok(()) => Ok(()),
        Err(err) => {
            telemetry_error!("{err}");
            Err(err)
        }
    }
}

fn run_build_script_inner() -> Result<(), Error> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        Error::Usage("CARGO_MANIFEST_DIR is not set. run_build_script is for Cargo build.rs".into())
    })?);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").map_err(|_| {
        Error::Usage("OUT_DIR is not set. run_build_script is for Cargo build.rs".into())
    })?);

    telemetry_info!(root = %root.display(), "kconfig build.rs starting in {}", root.display());

    let kconfig = resolve_kconfig(&root, None)?;
    let defconfig = resolve_defconfig(&root, None)?;
    let request = GenerateRequest::new(root, kconfig.clone(), defconfig.clone());
    let result = run(&request)?;

    let config_rs = out_dir.join("config.rs");
    let dotconfig_path = out_dir.join(".config");
    write_outputs(&result, &config_rs, &dotconfig_path)?;

    for cfg in &result.generated.rustc_check_cfgs {
        println!("cargo:rustc-check-cfg={cfg}");
    }
    for cfg in &result.generated.rustc_cfgs {
        println!("cargo:rustc-cfg={cfg}");
    }
    println!("cargo:rerun-if-env-changed=KCONFIG_DEFCONFIG");
    println!("cargo:rerun-if-changed={}", kconfig.display());
    println!("cargo:rerun-if-changed={}", defconfig.display());
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

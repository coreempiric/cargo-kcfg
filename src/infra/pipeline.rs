use crate::domain::{EvaluatedConfig, Limits, evaluate};
use crate::error::Error;
use crate::infra::codegen::{Generated, generate};
use crate::infra::defconfig::load_defconfig;
use crate::infra::kconfig::{LoadedKconfig, load_kconfig};
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

//! Load → evaluate → generate orchestration.
//!
//! [`Pipeline::run`] is the single method called after a
//! [`crate::infra::locator::ProjectLocator`] has produced a [`GenerateRequest`].

use crate::domain::{AssignmentSet, EvaluatedConfig, Evaluator, Limits};
use crate::error::Error;
use crate::infra::codegen::{CodeGenerator, Generated};
use crate::infra::defconfig::DefconfigLoader;
use crate::infra::kconfig::{KconfigLoader, LoadedKconfig};
use std::path::{Path, PathBuf};

/// Inputs for a full load → evaluate → generate run.
///
/// `root_dir` is the directory used to resolve `source` paths.
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub root_dir: PathBuf,
    pub kconfig: PathBuf,
    pub defconfig: PathBuf,
    pub limits: Limits,
}

/// Successful pipeline output: evaluated values plus generated artefacts.
#[derive(Debug, Clone)]
pub struct GenerateResult {
    pub evaluated: EvaluatedConfig,
    pub loaded_files: Vec<PathBuf>,
    pub generated: Generated,
}

impl GenerateRequest {
    /// Build a request with [`Limits::default`].
    pub fn new(root_dir: PathBuf, kconfig: PathBuf, defconfig: PathBuf) -> Self {
        Self {
            root_dir,
            kconfig,
            defconfig,
            limits: Limits::default(),
        }
    }
}

/// Orchestrates Kconfig load, assignment load, evaluation, and code generation.
///
/// This is the library entry after [`crate::ProjectLocator`] has resolved paths.
pub struct Pipeline {
    kconfig: KconfigLoader,
    defconfig: DefconfigLoader,
    codegen: CodeGenerator,
}

impl Pipeline {
    /// Create a pipeline with default loaders and generator.
    pub fn new() -> Self {
        Self {
            kconfig: KconfigLoader::new(),
            defconfig: DefconfigLoader::new(),
            codegen: CodeGenerator::new(),
        }
    }

    /// Parse definitions and assignments, evaluate, and generate artefacts.
    ///
    /// # Errors
    ///
    /// - [`Error::Io`] / [`Error::Parse`] / [`Error::Domain`] while loading
    ///   Kconfig or the defconfig.
    /// - [`Error::Validation`] if evaluation is incoherent.
    pub fn run(&self, request: &GenerateRequest) -> Result<GenerateResult, Error> {
        let LoadedKconfig {
            table,
            loaded_files,
        } = self
            .kconfig
            .load(&request.root_dir, &request.kconfig, request.limits)?;
        let assignments = self.defconfig.load(&request.defconfig, request.limits)?;
        let evaluated = Evaluator::new(request.limits)
            .evaluate(table, &assignments)
            .map_err(Error::Validation)?;
        let generated = self.codegen.generate(&evaluated);
        Ok(GenerateResult {
            evaluated,
            loaded_files,
            generated,
        })
    }

    /// Load `Kconfig` and evaluate `assignments` without writing artefacts.
    ///
    /// # Errors
    ///
    /// Same load failures as [`Pipeline::run`], then [`Error::Validation`].
    pub fn evaluate(
        &self,
        root_dir: &Path,
        kconfig: &Path,
        assignments: &AssignmentSet,
        limits: Limits,
    ) -> Result<EvaluatedConfig, Error> {
        let loaded = self.kconfig.load(root_dir, kconfig, limits)?;
        Evaluator::new(limits)
            .evaluate(loaded.table, assignments)
            .map_err(Error::Validation)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Writes `config.rs` and `.config` for a successful [`Pipeline`] run.
pub struct ArtifactWriter;

impl ArtifactWriter {
    /// Create a writer. The type is stateless.
    pub fn new() -> Self {
        Self
    }

    /// Write `config.rs` and `.config` next to `config_rs` / `dotconfig`.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] if a parent directory cannot be created or a write fails.
    pub fn write(
        &self,
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
        std::fs::write(config_rs, &result.generated.config_rs)
            .map_err(|e| Error::io(config_rs, e))?;
        std::fs::write(dotconfig, &result.generated.dotconfig)
            .map_err(|e| Error::io(dotconfig, e))?;
        Ok(())
    }
}

impl Default for ArtifactWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse definitions and assignments, evaluate, and generate artefacts.
///
/// Alias for [`Pipeline::run`].
///
/// # Errors
///
/// Same as [`Pipeline::run`].
pub fn run(request: &GenerateRequest) -> Result<GenerateResult, Error> {
    Pipeline::new().run(request)
}

/// Write `config.rs` and `.config`.
///
/// Alias for [`ArtifactWriter::write`].
///
/// # Errors
///
/// Same as [`ArtifactWriter::write`].
pub fn write_outputs(
    result: &GenerateResult,
    config_rs: &Path,
    dotconfig: &Path,
) -> Result<(), Error> {
    ArtifactWriter::new().write(result, config_rs, dotconfig)
}

//! Evaluate known-good or known-bad assignment files from unit tests.
//!
//! Compile-time constants come from
//! `pub mod config { cargo_kconfig_macros::include_config!(); }`.
//! Tests that need to **branch** on a configuration call [`ConfigTest`]:
//!
//! ```rust,no_run
//! use cargo_kconfig::domain::{IssueKind, Value};
//! use cargo_kconfig::{ConfigTest, Error};
//!
//! let kconfig = ConfigTest::discover().expect("workspace Kconfig");
//! match kconfig.evaluate("configs/cases/unmet_defconfig") {
//!     Ok(cfg) if cfg.get("UART") == Some(&Value::Bool(true)) => {
//!         // UART-on path
//!     }
//!     Ok(_) => {
//!         // coherent config with UART off
//!     }
//!     Err(Error::Validation(report)) if report.has_kind(IssueKind::UnmetDependency) => {
//!         // illegal defconfig path
//!     }
//!     Err(err) => panic!("{err}"),
//! }
//! ```

use crate::domain::{EvaluatedConfig, Limits};
use crate::error::Error;
use crate::infra::defconfig::DefconfigLoader;
use crate::infra::locator::ProjectLocator;
use crate::infra::pipeline::Pipeline;
use std::path::{Path, PathBuf};

/// Runtime Kconfig evaluation for unit tests.
pub struct ConfigTest {
    locator: ProjectLocator,
    pipeline: Pipeline,
    defconfig: DefconfigLoader,
    limits: Limits,
}

impl ConfigTest {
    /// Locate `Kconfig` from `CARGO_MANIFEST_DIR` (or the current directory),
    /// walking up to a workspace root when this package has no `Kconfig`.
    pub fn discover() -> Result<Self, Error> {
        let start = match std::env::var_os("CARGO_MANIFEST_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => std::env::current_dir().map_err(|e| {
                Error::Usage(format!(
                    "cannot determine the project directory: {e}. Call ConfigTest::at(root) with the workspace path"
                ))
            })?,
        };
        Ok(Self::from_locator(ProjectLocator::discover(start)?))
    }

    /// Evaluate against a known project root (the directory that contains `Kconfig`).
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self::from_locator(ProjectLocator::new(root.into()))
    }

    pub fn from_locator(locator: ProjectLocator) -> Self {
        Self {
            locator,
            pipeline: Pipeline::new(),
            defconfig: DefconfigLoader::new(),
            limits: Limits::default(),
        }
    }

    pub fn root(&self) -> &Path {
        self.locator.root()
    }

    /// Evaluate an assignment file. Relative paths are resolved from the
    /// project root, then from `configs/`.
    ///
    /// Returns [`Error::Validation`] for an illegal defconfig so tests can
    /// take the error path without failing the crate build.
    pub fn evaluate(&self, defconfig: impl AsRef<Path>) -> Result<EvaluatedConfig, Error> {
        let path = self.locator.named_defconfig(defconfig.as_ref())?;
        let assignments = self.defconfig.load(&path, self.limits)?;
        self.evaluate_assignments(&assignments)
    }

    /// Evaluate assignment text in the same format as a `*_defconfig` file.
    pub fn evaluate_text(&self, text: &str) -> Result<EvaluatedConfig, Error> {
        let assignments = self.defconfig.parse_text(text, self.limits)?;
        self.evaluate_assignments(&assignments)
    }

    fn evaluate_assignments(
        &self,
        assignments: &crate::domain::AssignmentSet,
    ) -> Result<EvaluatedConfig, Error> {
        let kconfig = self.locator.kconfig(None)?;
        self.pipeline
            .evaluate(self.locator.root(), &kconfig, assignments, self.limits)
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigTest;
    use crate::domain::{IssueKind, Value};
    use crate::error::Error;
    use std::fs;

    fn workspace_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"lib\"]\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("Kconfig"),
            "config BUS\n\
             \tbool\n\
             \tdefault n\n\
             \n\
             config UART\n\
             \tbool\n\
             \tdepends on BUS\n\
             \tdefault n\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("configs/cases")).unwrap();
        fs::write(
            dir.path().join("configs").join("qemu_defconfig"),
            "CONFIG_BUS=y\nCONFIG_UART=y\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("configs/cases").join("unmet_defconfig"),
            "CONFIG_UART=y\n",
        )
        .unwrap();
        let lib = dir.path().join("lib");
        fs::create_dir(&lib).unwrap();
        fs::write(
            lib.join("Cargo.toml"),
            "[package]\nname = \"lib\"\nversion = \"0.0.1\"\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn good_defconfig_enables_the_uart_path() {
        let dir = workspace_tree();
        let probe = ConfigTest::at(dir.path());
        let cfg = probe.evaluate("configs/qemu_defconfig").unwrap();
        assert_eq!(cfg.get("BUS"), Some(&Value::Bool(true)));
        assert_eq!(cfg.get("UART"), Some(&Value::Bool(true)));
    }

    #[test]
    fn bad_defconfig_takes_the_validation_error_path() {
        let dir = workspace_tree();
        let probe = ConfigTest::at(dir.path());
        match probe.evaluate("configs/cases/unmet_defconfig") {
            Err(Error::Validation(report)) => {
                assert!(report.has_kind(IssueKind::UnmetDependency), "{report}");
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn inline_bad_text_controls_test_flow() {
        let dir = workspace_tree();
        let probe = ConfigTest::at(dir.path());
        match probe.evaluate_text("CONFIG_UART=y\n") {
            Err(Error::Validation(report)) => {
                assert!(report.has_kind(IssueKind::UnmetDependency), "{report}");
            }
            Ok(_) => panic!("UART=y without BUS must fail"),
            Err(err) => panic!("{err}"),
        }
    }

    #[test]
    fn discover_from_member_sees_workspace_kconfig() {
        let dir = workspace_tree();
        let loc = crate::infra::locator::ProjectLocator::discover(dir.path().join("lib")).unwrap();
        let probe = ConfigTest::from_locator(loc);
        let cfg = probe.evaluate("qemu_defconfig").unwrap();
        assert_eq!(cfg.get("UART"), Some(&Value::Bool(true)));
    }
}

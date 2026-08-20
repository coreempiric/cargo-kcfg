//! Locate `Kconfig` and a single `*_defconfig` without crawling the tree.
//!
//! Used by [`crate::cli::CheckCommand`], [`crate::cli::BuildCommand`], [`crate::cli::TestCommand`], and by
//! [`crate::infra::build_script::BuildScript`] before [`crate::infra::pipeline::Pipeline`].

use crate::error::Error;
use crate::infra::pipeline::GenerateRequest;
use crate::telemetry_info;
use std::path::{Path, PathBuf};

/// Resolves the Kconfig root and the unique assignment file for a project.
pub struct ProjectLocator {
    root: PathBuf,
}

impl ProjectLocator {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Build a [`GenerateRequest`] from optional explicit paths.
    pub fn resolve(
        &self,
        kconfig: Option<&Path>,
        defconfig: Option<&Path>,
    ) -> Result<GenerateRequest, Error> {
        Ok(GenerateRequest::new(
            self.root.clone(),
            self.kconfig(kconfig)?,
            self.defconfig(defconfig)?,
        ))
    }

    /// Resolve the root Kconfig file.
    pub fn kconfig(&self, explicit: Option<&Path>) -> Result<PathBuf, Error> {
        let kconfig = match explicit {
            Some(path) => path.to_path_buf(),
            None => self.root.join("Kconfig"),
        };
        if !kconfig.is_file() {
            return Err(Error::Usage(format!(
                "Kconfig file not found: {}. Pass --kconfig PATH, or create that file",
                kconfig.display()
            )));
        }
        telemetry_info!(path = %kconfig.display(), "using Kconfig {}", kconfig.display());
        Ok(kconfig)
    }

    /// Resolve the assignment file: explicit path, `KCONFIG_DEFCONFIG`, or a unique match.
    pub fn defconfig(&self, explicit: Option<&Path>) -> Result<PathBuf, Error> {
        if let Some(path) = explicit {
            return self.require_file(path.to_path_buf());
        }
        if let Ok(name) = std::env::var("KCONFIG_DEFCONFIG") {
            let path = {
                let candidate = PathBuf::from(&name);
                if candidate.is_absolute() {
                    candidate
                } else {
                    self.root.join(candidate)
                }
            };
            telemetry_info!(
                path = %path.display(),
                "KCONFIG_DEFCONFIG is set; using {}",
                path.display()
            );
            return self.require_file(path);
        }
        self.unique_defconfig()
    }

    /// Find `defconfig` or exactly one `*_defconfig` in `root` and `root/configs`.
    pub fn unique_defconfig(&self) -> Result<PathBuf, Error> {
        let direct = self.root.join("defconfig");
        if direct.is_file() {
            telemetry_info!(
                path = %direct.display(),
                "using `defconfig` in the project root"
            );
            return Ok(direct);
        }

        let mut matches = self.collect_defconfigs(&self.root)?;
        let configs_dir = self.root.join("configs");
        if configs_dir.is_dir() {
            matches.extend(self.collect_defconfigs(&configs_dir)?);
        }
        matches.sort();
        matches.dedup();

        let listed = matches
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");

        match matches.len() {
            0 => Err(Error::Usage(
                "no `*_defconfig` or `defconfig` file found. Pass --defconfig PATH, set KCONFIG_DEFCONFIG, or add one in the project root or `configs/`".into(),
            )),
            1 => {
                let path = matches.remove(0);
                telemetry_info!(
                    path = %path.display(),
                    "found a single defconfig, using {}",
                    path.display()
                );
                Ok(path)
            }
            _ => Err(Error::Usage(format!(
                "multiple defconfig files found ({listed}). Choose one with --defconfig PATH, or set KCONFIG_DEFCONFIG to the filename"
            ))),
        }
    }

    fn require_file(&self, path: PathBuf) -> Result<PathBuf, Error> {
        if !path.is_file() {
            return Err(Error::Usage(format!(
                "defconfig file not found: {}. Pass --defconfig PATH or set KCONFIG_DEFCONFIG to an existing assignment file",
                path.display()
            )));
        }
        telemetry_info!(path = %path.display(), "using defconfig {}", path.display());
        Ok(path)
    }

    fn collect_defconfigs(&self, dir: &Path) -> Result<Vec<PathBuf>, Error> {
        let mut out = Vec::new();
        let entries = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| Error::io(dir, e))?;
            let path = entry.path();
            if path.is_file()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.ends_with("_defconfig")
            {
                telemetry_info!(path = %path.display(), "found defconfig candidate {}", path.display());
                out.push(path);
            }
        }
        Ok(out)
    }
}

/// Resolve the root Kconfig file.
pub fn resolve_kconfig(root: &Path, explicit: Option<&Path>) -> Result<PathBuf, Error> {
    ProjectLocator::new(root.to_path_buf()).kconfig(explicit)
}

/// Resolve the assignment file: explicit path, `KCONFIG_DEFCONFIG`, or a unique match.
pub fn resolve_defconfig(root: &Path, explicit: Option<&Path>) -> Result<PathBuf, Error> {
    ProjectLocator::new(root.to_path_buf()).defconfig(explicit)
}

/// Find `defconfig` or exactly one `*_defconfig` in `root` and `root/configs`.
pub fn find_unique_defconfig(root: &Path) -> Result<PathBuf, Error> {
    ProjectLocator::new(root.to_path_buf()).unique_defconfig()
}

#[cfg(test)]
mod tests {
    use super::ProjectLocator;
    use std::fs;

    #[test]
    fn unique_defconfig_is_selected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("board_defconfig"), "CONFIG_FOO=y\n").unwrap();
        let found = ProjectLocator::new(dir.path().to_path_buf())
            .unique_defconfig()
            .unwrap();
        assert!(found.ends_with("board_defconfig"));
    }

    #[test]
    fn multiple_defconfigs_are_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a_defconfig"), "").unwrap();
        fs::write(dir.path().join("b_defconfig"), "").unwrap();
        let err = ProjectLocator::new(dir.path().to_path_buf())
            .unique_defconfig()
            .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("multiple defconfig files"), "{text}");
        assert!(text.contains("KCONFIG_DEFCONFIG"), "{text}");
    }

    #[test]
    fn missing_kconfig_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = ProjectLocator::new(dir.path().to_path_buf())
            .kconfig(None)
            .unwrap_err();
        assert!(err.to_string().contains("Kconfig file not found"));
    }
}

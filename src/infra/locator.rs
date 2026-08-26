//! Locate `Kconfig` and a single `*_defconfig` without crawling the tree.
//!
//! Used by [`crate::cli::CheckCommand`], [`crate::cli::BuildCommand`], [`crate::cli::TestCommand`],
//! [`crate::infra::build_script::BuildScript`], and [`crate::infra::config_test::ConfigTest`]
//! before [`crate::infra::pipeline::Pipeline`].
//!
//! [`ProjectLocator::discover`] starts at a package directory (typically
//! `CARGO_MANIFEST_DIR`) and walks up to a parent that has both `Cargo.toml`
//! and `Kconfig`. That is the workspace-root layout: virtual workspace
//! `Cargo.toml`, root `Kconfig`, `configs/*_defconfig`, member crates in
//! subdirectories.

use crate::error::Error;
use crate::infra::pipeline::GenerateRequest;
use crate::telemetry_info;
use std::path::{Path, PathBuf};

/// Cap on parent-directory walks from a member crate to a workspace `Kconfig`.
const MAX_PARENT_WALK: usize = 64;

/// Resolves the Kconfig root and the unique assignment file for a project.
///
/// Does not crawl the tree for `Kconfig*` names; `source` following is done
/// later by [`crate::infra::kconfig::KconfigLoader`].
#[derive(Debug)]
pub struct ProjectLocator {
    root: PathBuf,
}

impl ProjectLocator {
    /// Use `root` as the project directory (must already contain `Kconfig`
    /// when [`Self::kconfig`] is called without an explicit path).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Find the Kconfig project root starting at `start`.
    ///
    /// Order:
    /// 1. `CARGO_WORKSPACE_DIR`, when that directory contains `Kconfig`.
    /// 2. Walk parents for a `[workspace]` `Cargo.toml` that also has `Kconfig`
    ///    (or a `Cargo.lock` next to `Kconfig`).
    /// 3. `start` itself, if it contains `Kconfig` (standalone package).
    ///
    /// Directories without `Cargo.toml` are skipped, not treated as an error.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] if no `Kconfig` is found at `start` or in a parent
    /// workspace.
    pub fn discover(start: impl Into<PathBuf>) -> Result<Self, Error> {
        let workspace_dir = std::env::var_os("CARGO_WORKSPACE_DIR").map(PathBuf::from);
        Self::discover_from(start.into(), workspace_dir)
    }

    pub(crate) fn discover_from(
        start: PathBuf,
        workspace_dir: Option<PathBuf>,
    ) -> Result<Self, Error> {
        if let Some(workspace) = workspace_dir.as_deref()
            && workspace.join("Kconfig").is_file()
        {
            telemetry_info!(
                path = %workspace.display(),
                "using CARGO_WORKSPACE_DIR Kconfig in {}",
                workspace.display()
            );
            return Ok(Self {
                root: workspace.to_path_buf(),
            });
        }

        let mut dir = start.clone();
        for _ in 0..MAX_PARENT_WALK {
            if let Ok(toml_content) = std::fs::read_to_string(dir.join("Cargo.toml"))
                && toml_content.contains("[workspace]")
                && dir.join("Kconfig").is_file()
            {
                telemetry_info!(
                    path = %dir.display(),
                    "using workspace Kconfig in {}",
                    dir.display()
                );
                return Ok(Self { root: dir });
            }

            if dir.join("Cargo.lock").is_file() && dir.join("Kconfig").is_file() {
                telemetry_info!(
                    path = %dir.display(),
                    "using lockfile-anchored Kconfig in {}",
                    dir.display()
                );
                return Ok(Self { root: dir });
            }

            let Some(parent) = dir.parent() else {
                break;
            };
            dir = parent.to_path_buf();
        }

        if start.join("Kconfig").is_file() {
            telemetry_info!(
                path = %start.display(),
                "using package-local Kconfig in {}",
                start.display()
            );
            return Ok(Self { root: start });
        }

        Err(Error::Usage(format!(
            "no Kconfig found at `{}` or in a parent workspace. Create Kconfig next to this package's Cargo.toml, place Kconfig at the workspace root, pass --root DIR, or pass --kconfig PATH",
            start.display()
        )))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Build a [`GenerateRequest`] from optional explicit paths.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] if `Kconfig` or the assignment file cannot be resolved.
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
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] if the path does not exist.
    pub fn kconfig(&self, explicit: Option<&Path>) -> Result<PathBuf, Error> {
        let kconfig = match explicit {
            Some(path) => {
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.root.join(path)
                }
            }
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
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] if the file is missing, or more than one `*_defconfig`
    /// is found in the root and `configs/`.
    pub fn defconfig(&self, explicit: Option<&Path>) -> Result<PathBuf, Error> {
        if let Some(path) = explicit {
            return self.named_defconfig(path);
        }
        if let Ok(name) = std::env::var("KCONFIG_DEFCONFIG") {
            let path = PathBuf::from(&name);
            telemetry_info!(
                name = name.as_str(),
                "KCONFIG_DEFCONFIG is set; resolving `{}`",
                name
            );
            return self.named_defconfig(&path);
        }
        self.unique_defconfig()
    }

    /// Resolve an assignment file relative to the project root.
    ///
    /// Tries `root/<path>`, then `root/configs/<filename>` when `path` is not
    /// absolute. Unit tests use this to feed a known-good or known-bad
    /// defconfig that is not the unique product file.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] if none of the candidate paths exist.
    pub fn named_defconfig(&self, given: &Path) -> Result<PathBuf, Error> {
        let mut candidates = Vec::new();
        if given.is_absolute() {
            candidates.push(given.to_path_buf());
        } else {
            candidates.push(self.root.join(given));
            if let Some(name) = given.file_name() {
                candidates.push(self.root.join("configs").join(name));
            }
        }
        for path in &candidates {
            if path.is_file() {
                telemetry_info!(path = %path.display(), "using defconfig {}", path.display());
                return Ok(path.clone());
            }
        }
        let listed = candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Err(Error::Usage(format!(
            "defconfig file not found (tried {listed}). Pass --defconfig PATH or set KCONFIG_DEFCONFIG to an existing assignment file"
        )))
    }

    /// Find `defconfig` or exactly one `*_defconfig` in `root` and `root/configs`.
    ///
    /// Nested directories such as `configs/cases/` are ignored so test-only
    /// assignment files are not treated as product files.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] if zero or more than one match is found.
    /// [`Error::Io`] if `root` or `configs/` cannot be read.
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
///
/// # Errors
///
/// Same as [`ProjectLocator::kconfig`].
pub fn resolve_kconfig(root: &Path, explicit: Option<&Path>) -> Result<PathBuf, Error> {
    ProjectLocator::new(root.to_path_buf()).kconfig(explicit)
}

/// Resolve the assignment file: explicit path, `KCONFIG_DEFCONFIG`, or a unique match.
///
/// # Errors
///
/// Same as [`ProjectLocator::defconfig`].
pub fn resolve_defconfig(root: &Path, explicit: Option<&Path>) -> Result<PathBuf, Error> {
    ProjectLocator::new(root.to_path_buf()).defconfig(explicit)
}

/// Find `defconfig` or exactly one `*_defconfig` in `root` and `root/configs`.
///
/// # Errors
///
/// Same as [`ProjectLocator::unique_defconfig`].
pub fn find_unique_defconfig(root: &Path) -> Result<PathBuf, Error> {
    ProjectLocator::new(root.to_path_buf()).unique_defconfig()
}

#[cfg(test)]
mod tests {
    use super::ProjectLocator;
    use std::fs;

    fn write_package_manifest(dir: &std::path::Path, name: &str) {
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.0.1\"\nedition = \"2024\"\n"),
        )
        .unwrap();
    }

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

    fn write_kconfig(dir: &std::path::Path) {
        fs::write(dir.join("Kconfig"), "config FOO\n\tbool\n").unwrap();
    }

    fn write_workspace_manifest(dir: &std::path::Path, members: &str) {
        fs::write(
            dir.join("Cargo.toml"),
            format!("[workspace]\nmembers = [{members}]\n"),
        )
        .unwrap();
    }

    #[test]
    fn discover_uses_cargo_workspace_dir_before_the_walk() {
        let dir = tempfile::tempdir().unwrap();
        write_kconfig(dir.path());
        let member = dir.path().join("crates").join("app");
        fs::create_dir_all(&member).unwrap();
        write_package_manifest(&member, "app");
        write_kconfig(&member);
        let loc = ProjectLocator::discover_from(member, Some(dir.path().to_path_buf())).unwrap();
        assert_eq!(loc.root(), dir.path());
    }

    #[test]
    fn discover_walks_to_workspace_manifest_with_kconfig() {
        let dir = tempfile::tempdir().unwrap();
        write_workspace_manifest(dir.path(), "\"lib\"");
        write_kconfig(dir.path());
        let lib = dir.path().join("lib");
        fs::create_dir(&lib).unwrap();
        write_package_manifest(&lib, "lib");
        let loc = ProjectLocator::discover(&lib).unwrap();
        assert_eq!(loc.root(), dir.path());
    }

    #[test]
    fn discover_workspace_manifest_wins_over_member_kconfig() {
        let dir = tempfile::tempdir().unwrap();
        write_workspace_manifest(dir.path(), "\"lib\"");
        fs::write(dir.path().join("Kconfig"), "config ROOT\n\tbool\n").unwrap();
        let lib = dir.path().join("lib");
        fs::create_dir(&lib).unwrap();
        write_package_manifest(&lib, "lib");
        fs::write(lib.join("Kconfig"), "config LOCAL\n\tbool\n").unwrap();
        let loc = ProjectLocator::discover(&lib).unwrap();
        assert_eq!(loc.root(), dir.path());
    }

    #[test]
    fn discover_walks_to_cargo_lock_next_to_kconfig() {
        let dir = tempfile::tempdir().unwrap();
        write_package_manifest(dir.path(), "pkg");
        fs::write(dir.path().join("Cargo.lock"), "").unwrap();
        write_kconfig(dir.path());
        let nested = dir.path().join("src");
        fs::create_dir(&nested).unwrap();
        let loc = ProjectLocator::discover(&nested).unwrap();
        assert_eq!(loc.root(), dir.path());
    }

    #[test]
    fn discover_skips_parents_without_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        write_workspace_manifest(dir.path(), "\"crates/app\"");
        write_kconfig(dir.path());
        let nested = dir.path().join("crates").join("app");
        fs::create_dir_all(&nested).unwrap();
        write_package_manifest(&nested, "app");
        let loc = ProjectLocator::discover(&nested).unwrap();
        assert_eq!(loc.root(), dir.path());
    }

    #[test]
    fn discover_skips_workspace_manifest_without_kconfig() {
        let dir = tempfile::tempdir().unwrap();
        write_workspace_manifest(dir.path(), "\"mid\"");
        write_kconfig(dir.path());
        let mid = dir.path().join("mid");
        fs::create_dir(&mid).unwrap();
        write_workspace_manifest(&mid, "\"lib\"");
        let lib = mid.join("lib");
        fs::create_dir(&lib).unwrap();
        write_package_manifest(&lib, "lib");
        let loc = ProjectLocator::discover(&lib).unwrap();
        assert_eq!(loc.root(), dir.path());
    }

    #[test]
    fn discover_falls_back_to_start_when_no_workspace_kconfig() {
        let dir = tempfile::tempdir().unwrap();
        write_package_manifest(dir.path(), "pkg");
        write_kconfig(dir.path());
        let loc = ProjectLocator::discover(dir.path()).unwrap();
        assert_eq!(loc.root(), dir.path());
    }

    #[test]
    fn discover_errors_when_no_kconfig_exists() {
        let dir = tempfile::tempdir().unwrap();
        let member = dir.path().join("lib");
        fs::create_dir(&member).unwrap();
        write_package_manifest(&member, "lib");
        let err = ProjectLocator::discover(&member).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("no Kconfig found"), "{text}");
        assert!(text.contains("workspace"), "{text}");
    }

    #[test]
    fn named_defconfig_finds_file_under_configs() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("configs")).unwrap();
        fs::write(
            dir.path().join("configs").join("qemu_defconfig"),
            "CONFIG_FOO=y\n",
        )
        .unwrap();
        let found = ProjectLocator::new(dir.path().to_path_buf())
            .named_defconfig(std::path::Path::new("qemu_defconfig"))
            .unwrap();
        assert!(found.ends_with("qemu_defconfig"));
        assert!(found.to_string_lossy().contains("configs"));
    }

    #[test]
    fn unique_defconfig_ignores_nested_case_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("configs/cases")).unwrap();
        fs::write(dir.path().join("configs").join("qemu_defconfig"), "").unwrap();
        fs::write(dir.path().join("configs/cases").join("unmet_defconfig"), "").unwrap();
        let found = ProjectLocator::new(dir.path().to_path_buf())
            .unique_defconfig()
            .unwrap();
        assert!(found.ends_with("qemu_defconfig"));
    }
}

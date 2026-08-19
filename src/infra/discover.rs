//! Locate `Kconfig` and a single `*_defconfig` without crawling the tree.

use crate::error::Error;
use crate::telemetry_info;
use std::path::{Path, PathBuf};

/// Resolve the root Kconfig file.
pub fn resolve_kconfig(root: &Path, explicit: Option<&Path>) -> Result<PathBuf, Error> {
    let kconfig = match explicit {
        Some(path) => path.to_path_buf(),
        None => root.join("Kconfig"),
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
pub fn resolve_defconfig(root: &Path, explicit: Option<&Path>) -> Result<PathBuf, Error> {
    if let Some(path) = explicit {
        return require_defconfig_file(path.to_path_buf());
    }
    if let Ok(name) = std::env::var("KCONFIG_DEFCONFIG") {
        let path = {
            let candidate = PathBuf::from(&name);
            if candidate.is_absolute() {
                candidate
            } else {
                root.join(candidate)
            }
        };
        telemetry_info!(
            path = %path.display(),
            "KCONFIG_DEFCONFIG is set; using {}",
            path.display()
        );
        return require_defconfig_file(path);
    }
    find_unique_defconfig(root)
}

fn require_defconfig_file(path: PathBuf) -> Result<PathBuf, Error> {
    if !path.is_file() {
        return Err(Error::Usage(format!(
            "defconfig file not found: {}. Pass --defconfig PATH or set KCONFIG_DEFCONFIG to an existing assignment file",
            path.display()
        )));
    }
    telemetry_info!(path = %path.display(), "using defconfig {}", path.display());
    Ok(path)
}

/// Find `defconfig` or exactly one `*_defconfig` in `root` and `root/configs`.
pub fn find_unique_defconfig(root: &Path) -> Result<PathBuf, Error> {
    let direct = root.join("defconfig");
    if direct.is_file() {
        telemetry_info!(
            path = %direct.display(),
            "using `defconfig` in the project root"
        );
        return Ok(direct);
    }

    let mut matches = collect_defconfigs(root)?;
    let configs_dir = root.join("configs");
    if configs_dir.is_dir() {
        matches.extend(collect_defconfigs(&configs_dir)?);
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

fn collect_defconfigs(dir: &Path) -> Result<Vec<PathBuf>, Error> {
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

#[cfg(test)]
mod tests {
    use super::{find_unique_defconfig, resolve_kconfig};
    use std::fs;

    #[test]
    fn unique_defconfig_is_selected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("board_defconfig"), "CONFIG_FOO=y\n").unwrap();
        let found = find_unique_defconfig(dir.path()).unwrap();
        assert!(found.ends_with("board_defconfig"));
    }

    #[test]
    fn multiple_defconfigs_are_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a_defconfig"), "").unwrap();
        fs::write(dir.path().join("b_defconfig"), "").unwrap();
        let err = find_unique_defconfig(dir.path()).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("multiple defconfig files"), "{text}");
        assert!(text.contains("KCONFIG_DEFCONFIG"), "{text}");
    }

    #[test]
    fn missing_kconfig_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_kconfig(dir.path(), None).unwrap_err();
        assert!(err.to_string().contains("Kconfig file not found"));
    }
}

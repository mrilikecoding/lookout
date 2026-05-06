//! Path allowlist for `show_image(path=...)`.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ImagePathAllowlist {
    roots: Vec<PathBuf>,
}

impl ImagePathAllowlist {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    pub fn default_roots() -> Self {
        let mut roots = Vec::new();
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            roots.push(home);
        }
        roots.push(std::env::temp_dir());
        Self { roots }
    }

    /// Returns the canonicalized path if it is inside one of the allowlist roots.
    pub fn check(&self, path: &Path) -> Result<PathBuf> {
        let canon = std::fs::canonicalize(path)
            .map_err(|_| Error::PathNotAllowed(path.to_path_buf()))?;
        for root in &self.roots {
            // Canonicalize the root too so we compare apples to apples
            // (macOS quirk: /tmp is a symlink to /private/tmp).
            let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            if canon.starts_with(&canon_root) {
                return Ok(canon);
            }
        }
        Err(Error::PathNotAllowed(canon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn allows_path_inside_root() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("x.png");
        std::fs::write(&f, b"PNG").unwrap();
        let al = ImagePathAllowlist::new(vec![dir.path().to_path_buf()]);
        let ok = al.check(&f).unwrap();
        let canon_dir = std::fs::canonicalize(dir.path()).unwrap();
        assert!(ok.starts_with(&canon_dir));
    }

    #[test]
    fn rejects_path_outside_roots() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let f = outside.path().join("x.png");
        std::fs::write(&f, b"PNG").unwrap();
        let al = ImagePathAllowlist::new(vec![dir.path().to_path_buf()]);
        assert!(matches!(al.check(&f), Err(Error::PathNotAllowed(_))));
    }
}

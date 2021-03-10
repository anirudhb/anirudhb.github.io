use std::path::{Path, PathBuf};

pub trait PathHelper {
    /// Attempts to canonicalize the path, otherwise
    /// returns it as is.
    fn maybe_canonicalize(&self) -> PathBuf;
    /// Attempts to join the given path with self,
    /// unless self is an absolute path.
    fn maybe_suffix(&self, p: &Path) -> PathBuf;
    /// Attempts to remove the given prefix from self,
    /// unless self is a relative path.
    fn maybe_unprefix(&self, p: &Path) -> &Path;
}

impl PathHelper for Path {
    fn maybe_canonicalize(&self) -> PathBuf {
        self.canonicalize().unwrap_or_else(|_| self.to_path_buf())
    }
    fn maybe_suffix(&self, p: &Path) -> PathBuf {
        if self.is_absolute() {
            self.to_path_buf()
        } else {
            p.join(self)
        }
    }
    fn maybe_unprefix(&self, p: &Path) -> &Path {
        if self.is_relative() {
            self.strip_prefix(p).unwrap_or(self)
        } else {
            self
        }
    }
}

use anyhow::Context;
use std::path::{Component, Path, PathBuf};

pub trait StorageBackend: Send + Sync {
    fn put(&self, relative_path: &str, bytes: &[u8]) -> anyhow::Result<()>;
    fn delete(&self, relative_path: &str) -> anyhow::Result<()>;
    fn resolve(&self, relative_path: &str) -> anyhow::Result<PathBuf>;
}

#[derive(Clone, Debug)]
pub struct LocalFsStorage {
    root: PathBuf,
}

impl LocalFsStorage {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn sanitize_relative_path(&self, relative_path: &str) -> anyhow::Result<PathBuf> {
        let path = Path::new(relative_path);
        if path.is_absolute() {
            anyhow::bail!("absolute paths are not allowed");
        }

        let mut clean = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => clean.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    anyhow::bail!("path traversal is not allowed");
                }
            }
        }

        if clean.as_os_str().is_empty() {
            anyhow::bail!("empty storage path");
        }

        Ok(clean)
    }
}

impl StorageBackend for LocalFsStorage {
    fn put(&self, relative_path: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let full_path = self.resolve(relative_path)?;
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("create parent directory for {}", full_path.display())
            })?;
        }
        std::fs::write(&full_path, bytes)
            .with_context(|| format!("write file {}", full_path.display()))?;
        Ok(())
    }

    fn delete(&self, relative_path: &str) -> anyhow::Result<()> {
        let full_path = self.resolve(relative_path)?;
        match std::fs::remove_file(&full_path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| format!("delete file {}", full_path.display())),
        }
    }

    fn resolve(&self, relative_path: &str) -> anyhow::Result<PathBuf> {
        let clean = self.sanitize_relative_path(relative_path)?;
        Ok(self.root.join(clean))
    }
}

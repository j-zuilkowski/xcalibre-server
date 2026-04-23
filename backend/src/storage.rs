use anyhow::Context;
use bytes::Bytes;
use std::path::{Component, Path, PathBuf};

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    async fn put(&self, relative_path: &str, bytes: Bytes) -> anyhow::Result<()>;
    async fn delete(&self, relative_path: &str) -> anyhow::Result<()>;
    async fn get_bytes(&self, relative_path: &str) -> anyhow::Result<Bytes>;
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

#[async_trait::async_trait]
impl StorageBackend for LocalFsStorage {
    async fn put(&self, relative_path: &str, bytes: Bytes) -> anyhow::Result<()> {
        let full_path = self.resolve(relative_path)?;
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create parent directory for {}", full_path.display()))?;
        }
        tokio::fs::write(&full_path, bytes)
            .await
            .with_context(|| format!("write file {}", full_path.display()))?;
        Ok(())
    }

    async fn delete(&self, relative_path: &str) -> anyhow::Result<()> {
        let full_path = self.resolve(relative_path)?;
        match tokio::fs::remove_file(&full_path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| format!("delete file {}", full_path.display())),
        }
    }

    async fn get_bytes(&self, relative_path: &str) -> anyhow::Result<Bytes> {
        let full_path = self.resolve(relative_path)?;
        let bytes = tokio::fs::read(&full_path)
            .await
            .with_context(|| format!("read file {}", full_path.display()))?;
        Ok(Bytes::from(bytes))
    }

    fn resolve(&self, relative_path: &str) -> anyhow::Result<PathBuf> {
        let clean = self.sanitize_relative_path(relative_path)?;
        Ok(self.root.join(clean))
    }
}

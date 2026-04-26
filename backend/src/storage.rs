//! Local filesystem storage backend.
//!
//! Implements [`StorageBackend`] for files stored under a configurable root directory.
//! The root directory is set from `app.storage_path` in config.
//!
//! # Path traversal prevention
//! All paths are validated via [`sanitize_relative_path`] before joining with the
//! root.  The sanitizer uses `Path::components()` to enumerate path segments and
//! rejects any `ParentDir` (`..`), `RootDir`, or `Prefix` components.  Windows
//! drive-letter prefixes (`C:\...`) are also detected and rejected.
//!
//! # Range request support
//! [`LocalFsStorage::get_range`] supports byte-range reads using `AsyncSeekExt` to
//! seek to the start offset and `read_exact` to read the requested slice.  A single
//! `metadata()` call is made at the beginning of each range request to learn the
//! total file size (or the caller may pass `total_length` to skip that syscall).
//!
//! # External systems
//! Local filesystem only.  See `storage_s3.rs` for the S3/MinIO/R2/B2 backend.

use anyhow::Context;
use bytes::Bytes;
use std::path::{Component, Path, PathBuf};

/// Sanitize a relative storage path against traversal attacks.
/// Returns a forward-slash-separated string safe to use as an S3 key
/// or as a relative path suffix for local storage.
/// Rejects absolute paths, ParentDir (..), RootDir, and Prefix components.
pub fn sanitize_relative_path(relative_path: &str) -> anyhow::Result<String> {
    if looks_like_windows_absolute_path(relative_path) {
        anyhow::bail!("absolute or prefixed paths are not allowed");
    }

    let path = Path::new(relative_path);
    if path.is_absolute() {
        anyhow::bail!("absolute paths are not allowed in storage keys");
    }

    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let s = part
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("non-UTF-8 path component"))?;
                parts.push(s.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                anyhow::bail!("path traversal is not allowed (.. component)");
            }
            Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("absolute or prefixed paths are not allowed");
            }
        }
    }

    if parts.is_empty() {
        anyhow::bail!("empty storage path");
    }

    Ok(parts.join("/"))
}

fn looks_like_windows_absolute_path(relative_path: &str) -> bool {
    let bytes = relative_path.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && bytes[0].is_ascii_alphabetic()
        && matches!(bytes[2], b'/' | b'\\')
}

/// Result of a ranged or full read from a storage backend.
///
/// `content_range` is `Some("bytes start-end/total")` for partial responses,
/// `None` for full reads.  `partial = true` indicates a range was served.
#[derive(Debug, Clone)]
pub struct GetRangeResult {
    pub bytes: Bytes,
    pub content_range: Option<String>,
    pub total_length: u64,
    pub partial: bool,
}

/// Abstraction over local-filesystem and S3-compatible object storage.
///
/// Implementations must be `Send + Sync` for use behind an `Arc` in `AppState`.
///
/// # `resolve` vs `get_range`
/// `resolve` returns a local `PathBuf` without I/O — only valid for local-FS storage.
/// S3 backends must return `Err` from `resolve`; callers should use `get_range` or
/// `get_bytes` for content access and `resolve_or_download_path` for extraction.
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    async fn put(&self, relative_path: &str, bytes: Bytes) -> anyhow::Result<()>;
    async fn delete(&self, relative_path: &str) -> anyhow::Result<()>;
    async fn file_size(&self, relative_path: &str) -> anyhow::Result<u64>;
    /// Read a byte range from the file. `range = None` reads the entire file.
    /// `total_length` may be passed if already known to skip a metadata syscall.
    async fn get_range(
        &self,
        relative_path: &str,
        range: Option<(u64, u64)>,
        total_length: Option<u64>,
    ) -> anyhow::Result<GetRangeResult>;
    async fn get_bytes(&self, relative_path: &str) -> anyhow::Result<Bytes> {
        Ok(self.get_range(relative_path, None, None).await?.bytes)
    }
    /// Return the absolute local path for a storage-relative path.
    ///
    /// Only valid for local-FS backends. S3 backends must return `Err`.
    fn resolve(&self, relative_path: &str) -> anyhow::Result<PathBuf>;
}

/// Local filesystem storage backend.
///
/// All stored files live under `root`. Paths are sanitized through
/// [`sanitize_relative_path`] before joining, preventing traversal outside `root`.
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

    async fn file_size(&self, relative_path: &str) -> anyhow::Result<u64> {
        let full_path = self.resolve(relative_path)?;
        let metadata = tokio::fs::metadata(&full_path)
            .await
            .with_context(|| format!("read file metadata {}", full_path.display()))?;
        Ok(metadata.len())
    }

    async fn get_range(
        &self,
        relative_path: &str,
        range: Option<(u64, u64)>,
        total_length: Option<u64>,
    ) -> anyhow::Result<GetRangeResult> {
        let full_path = self.resolve(relative_path)?;
        let total_length = match total_length {
            Some(total_length) => total_length,
            None => {
                let metadata = tokio::fs::metadata(&full_path)
                    .await
                    .with_context(|| format!("read file metadata {}", full_path.display()))?;
                metadata.len()
            }
        };

        match range {
            None => {
                let bytes = tokio::fs::read(&full_path)
                    .await
                    .with_context(|| format!("read file {}", full_path.display()))?;
                Ok(GetRangeResult {
                    bytes: Bytes::from(bytes),
                    content_range: None,
                    total_length,
                    partial: false,
                })
            }
            Some((start, end)) => {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};

                if total_length == 0 {
                    anyhow::bail!("invalid byte range for empty file");
                }
                let max_end = total_length.saturating_sub(1);
                if start > max_end {
                    anyhow::bail!(
                        "range start is beyond end of file: start={start}, len={total_length}"
                    );
                }

                let clamped_end = end.min(max_end);
                if clamped_end < start {
                    anyhow::bail!("invalid range: start={start}, end={clamped_end}");
                }

                let len_u64 = clamped_end - start + 1;
                let len = usize::try_from(len_u64).context("range is too large to allocate")?;
                let mut file = tokio::fs::File::open(&full_path)
                    .await
                    .with_context(|| format!("open file {}", full_path.display()))?;
                file.seek(std::io::SeekFrom::Start(start))
                    .await
                    .with_context(|| format!("seek file {}", full_path.display()))?;
                let mut buf = vec![0u8; len];
                file.read_exact(&mut buf)
                    .await
                    .with_context(|| format!("read range from {}", full_path.display()))?;

                Ok(GetRangeResult {
                    bytes: Bytes::from(buf),
                    content_range: Some(format!("bytes {start}-{clamped_end}/{total_length}")),
                    total_length,
                    partial: true,
                })
            }
        }
    }

    fn resolve(&self, relative_path: &str) -> anyhow::Result<PathBuf> {
        let clean = sanitize_relative_path(relative_path)?;
        Ok(self.root.join(clean))
    }
}

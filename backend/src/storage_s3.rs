//! S3-compatible object storage backend (AWS S3, MinIO, Cloudflare R2, Backblaze B2).
//!
//! Implements [`StorageBackend`] using the `aws-sdk-s3` crate.  All four providers
//! use the same S3 API; only `endpoint_url` differs (leave blank for AWS S3).
//!
//! # Path sanitization
//! [`S3Storage::s3_key`] calls [`sanitize_relative_path`] (Phase 16 Stage 1) which
//! uses `Path::components()` to strip any `..` components before building the S3
//! key.  This prevents path traversal via crafted filenames.
//!
//! # Range request limitation
//! S3 `GetObject` with a `Range` header streams the requested byte range from S3,
//! but the AWS SDK does not support true streaming with partial-content resumption.
//! The full requested range is buffered via `body.collect().into_bytes()` before
//! being returned.  For very large range requests this means the entire range is
//! loaded into memory.  Callers (the media streaming handler) should request
//! reasonably-sized ranges.
//!
//! # `resolve()` not supported
//! S3 objects have no local path.  `resolve()` always returns `Err` with a clear
//! message.  Callers that need a local file (e.g. text extraction) must use
//! `ingest::text::resolve_or_download_path` which downloads to a temp file.
//!
//! # `force_path_style`
//! Enabled for non-AWS endpoints (MinIO, R2, B2) which require path-style addressing.

use crate::{
    config::S3Section,
    storage::{sanitize_relative_path, GetRangeResult, StorageBackend},
};
use anyhow::Context;
use aws_credential_types::Credentials;
use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::{primitives::ByteStream, Client, Config};
use aws_smithy_http_client::{tls, Builder as SmithyHttpClientBuilder};
use bytes::Bytes;

#[derive(Clone, Debug)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    key_prefix: String,
}

impl S3Storage {
    pub async fn new(cfg: &S3Section) -> anyhow::Result<Self> {
        let creds = Credentials::new(
            cfg.access_key.clone(),
            cfg.secret_key.clone(),
            None,
            None,
            "xcalibre-server-config",
        );
        let http_client = SmithyHttpClientBuilder::new()
            .tls_provider(tls::Provider::S2nTls)
            .build_https();

        let mut builder = Config::builder()
            .http_client(http_client)
            .credentials_provider(creds)
            .region(aws_sdk_s3::config::Region::new(cfg.region.clone()))
            .behavior_version_latest();

        if !cfg.endpoint_url.trim().is_empty() {
            builder = builder
                .endpoint_url(cfg.endpoint_url.trim())
                .force_path_style(true);
        }

        let client = Client::from_conf(builder.build());
        Ok(Self {
            client,
            bucket: cfg.bucket.clone(),
            key_prefix: cfg.key_prefix.trim_end_matches('/').to_string(),
        })
    }

    /// Build the full S3 object key for a storage-relative path.
    ///
    /// Sanitizes `relative_path` via `sanitize_relative_path` to strip `..` components,
    /// then prepends `key_prefix` if configured.
    pub fn s3_key(&self, relative_path: &str) -> anyhow::Result<String> {
        let clean = sanitize_relative_path(relative_path)?;
        if self.key_prefix.is_empty() {
            Ok(clean)
        } else {
            Ok(format!(
                "{}/{}",
                self.key_prefix.trim_end_matches('/'),
                clean
            ))
        }
    }

    fn parse_total_length_from_content_range(content_range: &str) -> Option<u64> {
        let (_, total) = content_range.rsplit_once('/')?;
        total.parse().ok()
    }
}

#[async_trait::async_trait]
impl StorageBackend for S3Storage {
    async fn put(&self, relative_path: &str, bytes: Bytes) -> anyhow::Result<()> {
        let key = self.s3_key(relative_path)?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .with_context(|| format!("S3 PutObject {key}"))?;
        Ok(())
    }

    async fn delete(&self, relative_path: &str) -> anyhow::Result<()> {
        let key = self.s3_key(relative_path)?;
        match self
            .client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => {
                let service_err = err.into_service_error();
                let code = service_err.code().unwrap_or_default();
                if code == "NoSuchKey" || code == "NotFound" {
                    Ok(())
                } else {
                    Err(anyhow::Error::new(service_err)).with_context(|| {
                        format!(
                            "S3 DeleteObject failed for bucket={} key={key}",
                            self.bucket
                        )
                    })
                }
            }
        }
    }

    async fn file_size(&self, relative_path: &str) -> anyhow::Result<u64> {
        let key = self.s3_key(relative_path)?;
        let response = match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                let service_err = err.into_service_error();
                let code = service_err.code().unwrap_or_default();
                if code == "NoSuchKey" || code == "NotFound" {
                    anyhow::bail!("s3 object not found: {key}");
                }
                return Err(anyhow::Error::new(service_err))
                    .with_context(|| format!("S3 HeadObject {key}"));
            }
        };

        let content_length = response
            .content_length()
            .context("S3 HeadObject missing content length")?;
        if content_length < 0 {
            anyhow::bail!("S3 reported negative content length for {key}");
        }

        Ok(u64::try_from(content_length).context("convert S3 content length")?)
    }

    async fn get_range(
        &self,
        relative_path: &str,
        range: Option<(u64, u64)>,
        _total_length: Option<u64>,
    ) -> anyhow::Result<GetRangeResult> {
        let key = self.s3_key(relative_path)?;
        let mut request = self.client.get_object().bucket(&self.bucket).key(&key);
        if let Some((start, end)) = range {
            request = request.range(format!("bytes={start}-{end}"));
        }

        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                let service_err = err.into_service_error();
                let code = service_err.code().unwrap_or_default();
                if code == "NoSuchKey" || code == "NotFound" {
                    anyhow::bail!("s3 object not found: {key}");
                }
                return Err(anyhow::Error::new(service_err))
                    .with_context(|| format!("S3 GetObject {key}"));
            }
        };

        let content_range = response.content_range().map(ToString::to_string);
        let partial = content_range.is_some();
        let content_length = response
            .content_length()
            .and_then(|length| u64::try_from(length).ok())
            .unwrap_or(0);
        let total_length = content_range
            .as_deref()
            .and_then(Self::parse_total_length_from_content_range)
            .unwrap_or(content_length);
        let bytes = response
            .body
            .collect()
            .await
            .context("collect S3 response body")?
            .into_bytes();

        Ok(GetRangeResult {
            bytes,
            content_range,
            total_length,
            partial,
        })
    }

    fn resolve(&self, relative_path: &str) -> anyhow::Result<std::path::PathBuf> {
        anyhow::bail!(
            "resolve() is not supported for the S3 backend (path: {relative_path}). \
             Use get_range() or get_bytes() to retrieve file contents."
        )
    }
}

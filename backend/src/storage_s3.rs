use crate::{config::S3Section, storage::StorageBackend};
use anyhow::Context;
use aws_credential_types::Credentials;
use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::{primitives::ByteStream, Client, Config};
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
            "autolibre-config",
        );

        let mut builder = Config::builder()
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

    pub fn s3_key(&self, relative_path: &str) -> String {
        let normalized = relative_path.replace('\\', "/");
        let clean = normalized
            .split('/')
            .filter(|part| !part.is_empty() && *part != "..")
            .collect::<Vec<_>>()
            .join("/");

        if self.key_prefix.is_empty() {
            clean
        } else {
            format!("{}/{}", self.key_prefix, clean)
        }
    }
}

#[async_trait::async_trait]
impl StorageBackend for S3Storage {
    async fn put(&self, relative_path: &str, bytes: Bytes) -> anyhow::Result<()> {
        let key = self.s3_key(relative_path);
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
        let key = self.s3_key(relative_path);
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

    async fn get_bytes(&self, relative_path: &str) -> anyhow::Result<Bytes> {
        let key = self.s3_key(relative_path);
        let response = match self
            .client
            .get_object()
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
                    .with_context(|| format!("S3 GetObject {key}"));
            }
        };

        let bytes = response
            .body
            .collect()
            .await
            .context("collect S3 response body")?
            .into_bytes();

        Ok(bytes)
    }

    fn resolve(&self, relative_path: &str) -> anyhow::Result<std::path::PathBuf> {
        anyhow::bail!(
            "resolve() is not supported for the S3 backend (path: {relative_path}). \
             Use get_bytes() to retrieve file contents."
        )
    }
}

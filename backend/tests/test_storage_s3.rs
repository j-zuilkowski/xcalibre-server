#![allow(dead_code, unused_imports)]

use backend::{
    config::S3Section,
    storage::{LocalFsStorage, StorageBackend},
    storage_s3::S3Storage,
};
use bytes::Bytes;
use std::env;
use uuid::Uuid;

#[tokio::test]
async fn test_local_storage_resolve_returns_path() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let storage = LocalFsStorage::new(tempdir.path());

    storage
        .put("books/test.epub", Bytes::from_static(b"epub-bytes"))
        .await
        .expect("write file");

    let resolved = storage.resolve("books/test.epub").expect("resolve path");
    assert!(resolved.exists());
}

#[tokio::test]
async fn test_local_storage_get_bytes_returns_content() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let storage = LocalFsStorage::new(tempdir.path());

    storage
        .put("books/test.epub", Bytes::from_static(b"known-content"))
        .await
        .expect("write file");

    let bytes = storage
        .get_bytes("books/test.epub")
        .await
        .expect("read bytes");
    assert_eq!(bytes.as_ref(), b"known-content");
}

#[tokio::test]
async fn test_local_storage_delete_removes_file() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let storage = LocalFsStorage::new(tempdir.path());

    storage
        .put("books/test.epub", Bytes::from_static(b"to-delete"))
        .await
        .expect("write file");
    storage
        .delete("books/test.epub")
        .await
        .expect("delete file");

    let resolved = storage.resolve("books/test.epub").expect("resolve path");
    assert!(!resolved.exists());
}

#[tokio::test]
async fn test_local_storage_delete_missing_file_is_ok() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let storage = LocalFsStorage::new(tempdir.path());
    storage
        .delete("nonexistent/file.epub")
        .await
        .expect("delete missing file should succeed");
}

#[tokio::test]
async fn test_s3_resolve_returns_error() {
    let storage = S3Storage::new(&dummy_s3_config())
        .await
        .expect("create storage");
    assert!(storage.resolve("any/path").is_err());
}

#[tokio::test]
async fn test_s3_key_strips_traversal() {
    let storage = S3Storage::new(&dummy_s3_config())
        .await
        .expect("create storage");
    let key = storage.s3_key("../../etc/passwd");
    assert!(!key.contains(".."));
}

#[tokio::test]
async fn test_s3_key_applies_prefix() {
    let storage = S3Storage::new(&S3Section {
        bucket: "bucket".to_string(),
        region: "us-east-1".to_string(),
        endpoint_url: String::new(),
        access_key: "access".to_string(),
        secret_key: "secret".to_string(),
        key_prefix: "autolibre".to_string(),
    })
    .await
    .expect("create storage");

    let key = storage.s3_key("covers/ab/book.jpg");
    assert_eq!(key, "autolibre/covers/ab/book.jpg");
}

#[tokio::test]
#[ignore]
async fn test_s3_put_get_delete_roundtrip() {
    let Some(config) = s3_config_from_env() else {
        panic!(
            "missing S3 test config. set S3_TEST_BUCKET, S3_TEST_REGION, S3_TEST_ENDPOINT, S3_TEST_ACCESS_KEY, S3_TEST_SECRET_KEY"
        );
    };
    let storage = S3Storage::new(&config).await.expect("create storage");

    let key = format!("integration-tests/{}/book.epub", Uuid::new_v4());
    let payload = Bytes::from_static(b"s3-roundtrip-payload");

    storage
        .put(&key, payload.clone())
        .await
        .expect("put object");
    let fetched = storage.get_bytes(&key).await.expect("get object");
    assert_eq!(fetched, payload);

    storage.delete(&key).await.expect("delete object");
    let missing = storage.get_bytes(&key).await;
    assert!(missing.is_err());
}

fn dummy_s3_config() -> S3Section {
    S3Section {
        bucket: "dummy-bucket".to_string(),
        region: "us-east-1".to_string(),
        endpoint_url: "http://127.0.0.1:9000".to_string(),
        access_key: "dummy-access-key".to_string(),
        secret_key: "dummy-secret-key".to_string(),
        key_prefix: String::new(),
    }
}

fn s3_config_from_env() -> Option<S3Section> {
    let bucket = env::var("S3_TEST_BUCKET").ok()?;
    let region = env::var("S3_TEST_REGION").ok()?;
    let endpoint_url = env::var("S3_TEST_ENDPOINT").ok()?;
    let access_key = env::var("S3_TEST_ACCESS_KEY").ok()?;
    let secret_key = env::var("S3_TEST_SECRET_KEY").ok()?;
    let key_prefix = env::var("S3_TEST_KEY_PREFIX").unwrap_or_default();

    Some(S3Section {
        bucket,
        region,
        endpoint_url,
        access_key,
        secret_key,
        key_prefix,
    })
}

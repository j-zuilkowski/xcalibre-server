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
async fn test_local_storage_get_range_returns_partial_bytes() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let storage = LocalFsStorage::new(tempdir.path());
    let content: Vec<u8> = (0..1024).map(|i| (i % 251) as u8).collect();

    storage
        .put("books/range.bin", Bytes::from(content.clone()))
        .await
        .expect("write file");

    let result = storage
        .get_range("books/range.bin", Some((0, 511)))
        .await
        .expect("read range");

    assert_eq!(result.bytes.len(), 512);
    assert_eq!(result.bytes.as_ref(), &content[0..512]);
    assert_eq!(result.content_range.as_deref(), Some("bytes 0-511/1024"));
    assert_eq!(result.total_length, 1024);
    assert!(result.partial);
}

#[tokio::test]
async fn test_local_storage_get_range_open_end() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let storage = LocalFsStorage::new(tempdir.path());
    let content: Vec<u8> = (0..100).map(|i| i as u8).collect();

    storage
        .put("books/open-end.bin", Bytes::from(content.clone()))
        .await
        .expect("write file");

    let result = storage
        .get_range("books/open-end.bin", Some((50, u64::MAX)))
        .await
        .expect("read open-end range");

    assert_eq!(result.bytes.len(), 50);
    assert_eq!(result.bytes.as_ref(), &content[50..100]);
    assert_eq!(result.content_range.as_deref(), Some("bytes 50-99/100"));
    assert_eq!(result.total_length, 100);
    assert!(result.partial);
}

#[tokio::test]
async fn test_local_storage_get_range_none_returns_full() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let storage = LocalFsStorage::new(tempdir.path());
    let content = b"full-content".to_vec();

    storage
        .put("books/full.bin", Bytes::from(content.clone()))
        .await
        .expect("write file");

    let result = storage
        .get_range("books/full.bin", None)
        .await
        .expect("read full range");

    assert_eq!(result.bytes.as_ref(), content.as_slice());
    assert_eq!(result.total_length, content.len() as u64);
    assert!(!result.partial);
    assert_eq!(result.content_range, None);
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

#[tokio::test]
#[ignore]
async fn test_s3_get_range_passes_range_header() {
    let Some(config) = s3_config_from_env() else {
        panic!(
            "missing S3 test config. set S3_TEST_BUCKET, S3_TEST_REGION, S3_TEST_ENDPOINT, S3_TEST_ACCESS_KEY, S3_TEST_SECRET_KEY"
        );
    };
    let storage = S3Storage::new(&config).await.expect("create storage");

    let key = format!("integration-tests/{}/range.bin", Uuid::new_v4());
    let payload: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();

    storage
        .put(&key, Bytes::from(payload.clone()))
        .await
        .expect("put object");

    let result = storage
        .get_range(&key, Some((0, 1023)))
        .await
        .expect("range get object");
    assert_eq!(result.bytes.len(), 1024);
    assert_eq!(result.bytes.as_ref(), &payload[0..1024]);
    assert!(result.partial);

    storage.delete(&key).await.expect("delete object");
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

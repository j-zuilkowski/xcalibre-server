#![allow(dead_code, unused_imports)]

mod common;

#[tokio::test]
#[ignore]
async fn test_list_books_empty_library() { todo!() }

#[tokio::test]
#[ignore]
async fn test_list_books_pagination() { todo!() }

#[tokio::test]
#[ignore]
async fn test_list_books_filter_by_author() { todo!() }

#[tokio::test]
#[ignore]
async fn test_list_books_filter_by_tag() { todo!() }

#[tokio::test]
#[ignore]
async fn test_list_books_sort_by_title() { todo!() }

#[tokio::test]
#[ignore]
async fn test_list_books_since_returns_only_modified() { todo!() }

#[tokio::test]
#[ignore]
async fn test_get_book_returns_full_relations() { todo!() }

#[tokio::test]
#[ignore]
async fn test_get_book_not_found_returns_404() { todo!() }

#[tokio::test]
#[ignore]
async fn test_upload_epub_extracts_metadata() { todo!() }

#[tokio::test]
#[ignore]
async fn test_upload_epub_extracts_cover() { todo!() }

#[tokio::test]
#[ignore]
async fn test_upload_pdf_no_cover_ok() { todo!() }

#[tokio::test]
#[ignore]
async fn test_upload_unknown_format_returns_422() { todo!() }

#[tokio::test]
#[ignore]
async fn test_upload_magic_bytes_mismatch_returns_422() { todo!() }

#[tokio::test]
#[ignore]
async fn test_upload_duplicate_isbn_returns_409() { todo!() }

#[tokio::test]
#[ignore]
async fn test_upload_requires_upload_permission() { todo!() }

#[tokio::test]
#[ignore]
async fn test_patch_book_updates_fields() { todo!() }

#[tokio::test]
#[ignore]
async fn test_patch_book_writes_audit_log() { todo!() }

#[tokio::test]
#[ignore]
async fn test_patch_book_not_found_returns_404() { todo!() }

#[tokio::test]
#[ignore]
async fn test_delete_book_removes_files() { todo!() }

#[tokio::test]
#[ignore]
async fn test_delete_book_requires_admin() { todo!() }


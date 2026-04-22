#![allow(dead_code, unused_imports)]

use backend::ingest::text;
use std::{fs, path::Path};

#[test]
fn test_mobi_list_chapters_returns_chapters() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal.mobi");

    let chapters = text::list_chapters(&path, "mobi").expect("list chapters");

    assert!(!chapters.is_empty());
    assert!(chapters
        .iter()
        .all(|chapter| !chapter.title.trim().is_empty() && chapter.word_count > 0));
}

#[test]
fn test_mobi_extract_text_full_returns_content() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal.mobi");

    let full = text::extract_text(&path, "mobi", None).expect("extract mobi full");

    assert!(!full.trim().is_empty());
    assert!(full.contains("Lord") || full.contains("Rings"));
}

#[test]
fn test_mobi_extract_text_by_chapter() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal.mobi");

    let full = text::extract_text(&path, "mobi", None).expect("extract mobi full");
    let first = text::extract_text(&path, "mobi", Some(0)).expect("extract mobi chapter");

    assert!(!first.trim().is_empty());
    assert!(first.len() < full.len());
}

#[test]
fn test_txt_extract_text_returns_content() {
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    let path = temp.path();
    let content = "alpha beta gamma delta";
    fs::write(path, content).expect("write txt fixture");

    let chapters = text::list_chapters(path, "txt").expect("txt list chapters");
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].title, "Full Text");
    assert_eq!(chapters[0].word_count, content.split_whitespace().count());

    let extracted = text::extract_text(path, "txt", None).expect("txt extract text");
    assert_eq!(extracted, content);
}

#[test]
fn test_unknown_format_returns_empty() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal.epub");

    let chapters = text::list_chapters(&path, "cbz").expect("list chapters for unknown format");
    let extracted = text::extract_text(&path, "cbz", None).expect("extract unknown format");

    assert!(chapters.is_empty());
    assert!(extracted.is_empty());
}

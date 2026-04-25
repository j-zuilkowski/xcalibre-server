#![allow(dead_code, unused_imports)]

mod common;

use xs_migrate::calibre::reader::CalibreReader;

#[test]
fn test_reader_loads_books() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open calibre fixture");
    let entries = reader.read_all_entries().expect("read calibre entries");

    assert_eq!(entries.len(), 3);

    let first = &entries[0];
    assert_eq!(first.book.id, 1);
    assert_eq!(first.book.title, "Cover Book");
    assert_eq!(first.book.sort, "Cover Book");
    assert_eq!(first.book.author_sort, "Author One");
    assert_eq!(
        first.book.pubdate.as_deref(),
        Some("2020-01-01T00:00:00+00:00")
    );
    assert_eq!(first.book.series_index, Some(1.0));
    assert_eq!(first.book.rating, Some(8));
    assert!(first.book.has_cover);
    assert_eq!(first.book.last_modified, "2024-01-01T10:00:00+00:00");
}

#[test]
fn test_reader_loads_authors() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open calibre fixture");
    let entries = reader.read_all_entries().expect("read calibre entries");

    let names: Vec<&str> = entries
        .iter()
        .map(|entry| entry.authors[0].name.as_str())
        .collect();
    assert_eq!(names, vec!["Author One", "Author Two", "Author Three"]);
    assert_eq!(entries[0].authors[0].sort, "One, Author");
}

#[test]
fn test_reader_loads_formats() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open calibre fixture");
    let entries = reader.read_all_entries().expect("read calibre entries");

    let first_format = &entries[0].formats[0];
    assert_eq!(first_format.book_id, 1);
    assert_eq!(first_format.format, "EPUB");
    assert_eq!(first_format.name, "cover-book");
    assert_eq!(first_format.uncompressed_size, Some(111));

    let third_format = &entries[2].formats[0];
    assert_eq!(third_format.format, "MOBI");
    let mobi_path = reader.file_path(&entries[2], third_format);
    assert_eq!(
        mobi_path,
        library
            .path()
            .join("Author Three")
            .join("Mobi Book (3)")
            .join("mobi-book.mobi")
    );
    assert!(mobi_path.exists());
}

#[test]
fn test_reader_loads_identifiers() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open calibre fixture");
    let entries = reader.read_all_entries().expect("read calibre entries");

    let second = &entries[1];
    assert_eq!(second.identifiers.len(), 1);
    assert_eq!(second.identifiers[0].book_id, 2);
    assert_eq!(second.identifiers[0].id_type, "isbn");
    assert_eq!(second.identifiers[0].value, "9780000000002");
}

#[test]
fn test_reader_loads_comments() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open calibre fixture");
    let entries = reader.read_all_entries().expect("read calibre entries");

    assert_eq!(
        entries[0].comment.as_ref().map(|c| c.text.as_str()),
        Some("Cover book description")
    );
    assert_eq!(
        entries[1].comment.as_ref().map(|c| c.text.as_str()),
        Some("ISBN book description")
    );
    assert_eq!(
        entries[2].comment.as_ref().map(|c| c.text.as_str()),
        Some("MOBI book description")
    );
}

#[test]
fn test_reader_loads_series() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open calibre fixture");
    let entries = reader.read_all_entries().expect("read calibre entries");

    assert_eq!(
        entries[0].series.as_ref().map(|s| s.name.as_str()),
        Some("Series A")
    );
    assert_eq!(
        entries[1].series.as_ref().map(|s| s.sort.as_str()),
        Some("Series A")
    );
    assert!(entries[2].series.is_none());
}

#[test]
fn test_reader_loads_tags() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open calibre fixture");
    let entries = reader.read_all_entries().expect("read calibre entries");

    assert_eq!(entries[0].tags[0].name, "Fiction");
    assert_eq!(entries[1].tags[0].name, "Reference");
    assert_eq!(entries[2].tags[0].name, "Fiction");

    let cover_path = reader.cover_path(&entries[0]).expect("cover path");
    assert_eq!(
        cover_path,
        library
            .path()
            .join("Author One")
            .join("Cover Book (1)")
            .join("cover.jpg")
    );
    assert!(cover_path.exists());
    assert_eq!(reader.cover_path(&entries[1]), None);
}

use crate::AppError;
use csv::{ReaderBuilder, StringRecord};
use std::io::Cursor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoodreadsRow {
    pub title: String,
    pub author: String,
    pub my_rating: u8,
    pub date_read: Option<String>,
    pub bookshelves: Vec<String>,
    pub exclusive_shelf: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorygraphRow {
    pub title: String,
    pub authors: String,
    pub read_status: String,
    pub star_rating: Option<f32>,
    pub date_finished: Option<String>,
    pub tags: Vec<String>,
}

pub fn parse_goodreads_csv(bytes: &[u8]) -> Result<Vec<GoodreadsRow>, AppError> {
    let mut reader = csv_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|_| AppError::Unprocessable)?
        .clone();

    let title_idx = header_index(&headers, "Title")?;
    let author_idx = header_index(&headers, "Author")?;
    let rating_idx = header_index(&headers, "My Rating")?;
    let date_read_idx = header_index(&headers, "Date Read")?;
    let bookshelves_idx = header_index(&headers, "Bookshelves")?;
    let shelf_idx = header_index(&headers, "Exclusive Shelf")?;

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|_| AppError::Unprocessable)?;
        if record_is_empty(&record) {
            continue;
        }

        rows.push(GoodreadsRow {
            title: field(&record, title_idx),
            author: field(&record, author_idx),
            my_rating: field(&record, rating_idx).parse::<u8>().unwrap_or(0),
            date_read: optional_field(&record, date_read_idx),
            bookshelves: split_csv_list(&field(&record, bookshelves_idx)),
            exclusive_shelf: field(&record, shelf_idx).to_lowercase(),
        });
    }

    Ok(rows)
}

pub fn parse_storygraph_csv(bytes: &[u8]) -> Result<Vec<StorygraphRow>, AppError> {
    let mut reader = csv_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|_| AppError::Unprocessable)?
        .clone();

    let title_idx = header_index(&headers, "Title")?;
    let authors_idx = header_index(&headers, "Authors")?;
    let read_status_idx = header_index(&headers, "Read Status")?;
    let star_rating_idx = header_index(&headers, "Star Rating (x/5)")?;
    let date_finished_idx = header_index(&headers, "Last Date Read")?;
    let tags_idx = header_index(&headers, "Tags")?;

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|_| AppError::Unprocessable)?;
        if record_is_empty(&record) {
            continue;
        }

        rows.push(StorygraphRow {
            title: field(&record, title_idx),
            authors: field(&record, authors_idx),
            read_status: field(&record, read_status_idx).to_lowercase(),
            star_rating: optional_field(&record, star_rating_idx)
                .and_then(|value| value.parse::<f32>().ok()),
            date_finished: optional_field(&record, date_finished_idx),
            tags: split_csv_list(&field(&record, tags_idx)),
        });
    }

    Ok(rows)
}

fn csv_reader(bytes: &[u8]) -> csv::Reader {
    ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(Cursor::new(bytes))
}

fn header_index(headers: &StringRecord, name: &str) -> Result<usize, AppError> {
    headers
        .iter()
        .position(|header| header.trim() == name)
        .ok_or(AppError::Unprocessable)
}

fn field(record: &StringRecord, index: usize) -> String {
    record.get(index).unwrap_or_default().trim().to_string()
}

fn optional_field(record: &StringRecord, index: usize) -> Option<String> {
    let value = field(record, index);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn split_csv_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn record_is_empty(record: &StringRecord) -> bool {
    record.iter().all(|field| field.trim().is_empty())
}

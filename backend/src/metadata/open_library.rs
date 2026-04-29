use super::MetadataCandidate;
use serde_json::Value;
use std::time::Duration;

fn cover_url_for_id(cover_i: i64) -> String {
    format!("https://covers.openlibrary.org/b/id/{cover_i}-L.jpg")
}

pub async fn search(query: &str) -> anyhow::Result<Vec<MetadataCandidate>> {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return Ok(vec![]),
    };

    let url = match reqwest::Url::parse_with_params(
        "https://openlibrary.org/search.json",
        &[
            ("q", query),
            ("limit", "10"),
            ("fields", "key,title,author_name,first_publish_year,isbn,cover_i,publisher"),
        ],
    ) {
        Ok(url) => url,
        Err(_) => return Ok(vec![]),
    };

    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(_) => return Ok(vec![]),
    };

    let response = match response.error_for_status() {
        Ok(response) => response,
        Err(_) => return Ok(vec![]),
    };

    let payload: Value = match response.json().await {
        Ok(payload) => payload,
        Err(_) => return Ok(vec![]),
    };

    let mut results = Vec::new();
    let Some(docs) = payload.get("docs").and_then(Value::as_array) else {
        return Ok(results);
    };

    for doc in docs {
        let Some(external_id) = doc.get("key").and_then(Value::as_str) else {
            continue;
        };
        let Some(title) = doc.get("title").and_then(Value::as_str) else {
            continue;
        };

        let authors = doc
            .get("author_name")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let description = None;
        let publisher = doc
            .get("publisher")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let published_date = doc
            .get("first_publish_year")
            .and_then(Value::as_i64)
            .map(|year| year.to_string());

        let mut isbn_13 = None;
        let mut isbn_10 = None;
        if let Some(isbn_values) = doc.get("isbn").and_then(Value::as_array) {
            for isbn in isbn_values {
                let Some(value) = isbn.as_str() else {
                    continue;
                };
                let digits_only: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits_only.len() == 13 && isbn_13.is_none() {
                    isbn_13 = Some(value.to_string());
                } else if digits_only.len() == 10 && isbn_10.is_none() {
                    isbn_10 = Some(value.to_string());
                }
            }
        }

        let thumbnail_url = doc
            .get("cover_i")
            .and_then(Value::as_i64)
            .map(|cover_i| format!("https://covers.openlibrary.org/b/id/{cover_i}-M.jpg"));
        let cover_url = doc
            .get("cover_i")
            .and_then(Value::as_i64)
            .map(cover_url_for_id);

        results.push(MetadataCandidate {
            source: "open_library".to_string(),
            external_id: external_id.to_string(),
            title: title.to_string(),
            authors,
            description,
            publisher,
            published_date,
            isbn_13,
            isbn_10,
            thumbnail_url,
            cover_url,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::{cover_url_for_id, search};

    #[tokio::test]
    async fn test_open_library_search_returns_vec() {
        let result = search("Lord of the Rings Tolkien").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_library_cover_url_format() {
        let url = cover_url_for_id(8406786);
        assert_eq!(url, "https://covers.openlibrary.org/b/id/8406786-L.jpg");
    }
}

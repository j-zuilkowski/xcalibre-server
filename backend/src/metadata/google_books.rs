use super::MetadataCandidate;
use serde_json::Value;
use std::time::Duration;

fn strip_edge_curl(url: &str) -> String {
    url.replace("&edge=curl", "").replace("edge=curl&", "")
}

fn upgrade_to_https(url: &str) -> String {
    if url.starts_with("http://") {
        url.replacen("http://", "https://", 1)
    } else {
        url.to_string()
    }
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
        "https://www.googleapis.com/books/v1/volumes",
        &[
            ("q", query),
            ("maxResults", "10"),
            ("printType", "books"),
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
    let Some(items) = payload.get("items").and_then(Value::as_array) else {
        return Ok(results);
    };

    for item in items {
        let Some(external_id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(volume_info) = item.get("volumeInfo").and_then(Value::as_object) else {
            continue;
        };
        let Some(title) = volume_info.get("title").and_then(Value::as_str) else {
            continue;
        };

        let authors = volume_info
            .get("authors")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let description = volume_info
            .get("description")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let publisher = volume_info
            .get("publisher")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let published_date = volume_info
            .get("publishedDate")
            .and_then(Value::as_str)
            .map(ToString::to_string);

        let mut isbn_13 = None;
        let mut isbn_10 = None;
        if let Some(identifiers) = volume_info
            .get("industryIdentifiers")
            .and_then(Value::as_array)
        {
            for identifier in identifiers {
                let Some(id_type) = identifier.get("type").and_then(Value::as_str) else {
                    continue;
                };
                let Some(value) = identifier.get("identifier").and_then(Value::as_str) else {
                    continue;
                };

                match id_type {
                    "ISBN_13" if isbn_13.is_none() => isbn_13 = Some(value.to_string()),
                    "ISBN_10" if isbn_10.is_none() => isbn_10 = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        let thumbnail_url = volume_info
            .get("imageLinks")
            .and_then(Value::as_object)
            .and_then(|image_links| image_links.get("thumbnail"))
            .and_then(Value::as_str)
            .map(strip_edge_curl)
            .map(|url| upgrade_to_https(&url));

        let cover_url = thumbnail_url
            .as_ref()
            .map(|url| url.replace("zoom=1", "zoom=3"));

        results.push(MetadataCandidate {
            source: "google_books".to_string(),
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
    use super::{search, strip_edge_curl, upgrade_to_https};

    #[tokio::test]
    async fn test_google_books_search_returns_vec() {
        let result = search("Dune Herbert").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_google_books_strips_edge_curl_param() {
        let url = "https://books.google.com/thumbnail?zoom=1&edge=curl";
        let cleaned = strip_edge_curl(url);
        assert!(!cleaned.contains("edge=curl"));
    }

    #[test]
    fn test_google_books_upgrades_http_to_https() {
        let url = "http://books.google.com/thumbnail?zoom=1";
        let upgraded = upgrade_to_https(url);
        assert!(upgraded.starts_with("https://"));
    }
}

#![allow(dead_code, unused_imports)]

mod common;

use backend::{
    config::AppConfig,
    llm::{
        chat::ChatClient,
        synthesize::{synthesize, SynthesisChunk},
    },
};
use common::TestContext;
use serde_json::json;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn test_synthesize_returns_output_and_sources() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "Run the verification commands, then roll back if checks fail."
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let client = chat_client(&mock_server);
    let chunks = vec![sample_chunk(
        "Oracle Database 19c",
        Some("Backup > Restore"),
        "Restore the control file before opening the database.",
    )];

    let result = synthesize(
        Some(&client),
        true,
        "Restore the database safely",
        "runsheet",
        None,
        chunks,
        43,
    )
    .await
    .expect("synthesize");

    assert_eq!(result.query, "Restore the database safely");
    assert_eq!(result.format, "runsheet");
    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.sources[0].book_title, "Oracle Database 19c");
    assert_eq!(result.sources[0].heading_path.as_deref(), Some("Backup > Restore"));
    assert_eq!(result.output, "Run the verification commands, then roll back if checks fail.");
    assert!(!result.synthesis_unavailable);
    assert_eq!(result.retrieval_ms, 43);
}

#[tokio::test]
async fn test_synthesize_sources_are_grounded_in_chunks() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "Grounded synthesis."
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let client = chat_client(&mock_server);
    let chunks = vec![
        sample_chunk(
            "Oracle Database 19c",
            Some("Recovery > Datafiles"),
            "Check the alert log for ORA-01555."
        ),
        sample_chunk(
            "Oracle Database 19c",
            Some("Recovery > Undo"),
            "Increase undo retention when needed."
        ),
    ];

    let result = synthesize(
        Some(&client),
        true,
        "Fix snapshot too old failures",
        "research-synthesis",
        None,
        chunks.clone(),
        12,
    )
    .await
    .expect("synthesize");

    assert_eq!(result.sources.len(), 2);
    let request = received_request(&mock_server).await;
    let user_message = request["messages"][1]["content"]
        .as_str()
        .expect("user message");
    assert!(user_message.contains("[Source: Oracle Database 19c > Recovery > Datafiles]"));
    assert!(user_message.contains("Check the alert log for ORA-01555."));
    assert!(user_message.contains("[Source: Oracle Database 19c > Recovery > Undo]"));
    assert!(user_message.contains("Increase undo retention when needed."));
}

#[tokio::test]
async fn test_synthesize_runsheet_format_contains_steps() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "Runsheet output."
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let client = chat_client(&mock_server);
    let chunks = vec![sample_chunk(
        "Oracle Database 19c",
        Some("Administration > Backup"),
        "Use RMAN to validate the backup set.",
    )];

    let _ = synthesize(
        Some(&client),
        true,
        "Verify a backup workflow",
        "runsheet",
        None,
        chunks,
        21,
    )
    .await
    .expect("synthesize");

    let request = received_request(&mock_server).await;
    let user_message = request["messages"][1]["content"]
        .as_str()
        .expect("user message");
    assert!(user_message.contains("Prerequisites"));
    assert!(user_message.contains("numbered Steps"));
    assert!(user_message.contains("Rollback procedure"));
    assert!(user_message.contains("Cite the source chunk for each step"));
}

#[tokio::test]
async fn test_synthesize_spice_format_output_is_valid_spice() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "* RC low-pass\nV1 in 0 DC 5\nR1 in out 1k\nC1 out 0 10u\n.op\n.tran 1ms 10ms\n.end"
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let client = chat_client(&mock_server);
    let result = synthesize(
        Some(&client),
        true,
        "Generate a simple RC filter netlist",
        "spice-netlist",
        None,
        vec![sample_chunk(
            "Analog Handbook",
            Some("Filters"),
            "A simple RC low-pass filter uses one resistor and one capacitor.",
        )],
        8,
    )
    .await
    .expect("synthesize");

    assert!(result.output.contains(".op"));
    assert!(result.output.contains(".tran"));
    assert!(result.output.contains("R1"));
}

#[tokio::test]
async fn test_synthesize_custom_format_uses_custom_prompt() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "Custom synthesis."
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let client = chat_client(&mock_server);
    let custom_prompt = "Only produce bullet points with exactly five items.";
    let _ = synthesize(
        Some(&client),
        true,
        "Summarize the procedure",
        "custom",
        Some(custom_prompt),
        vec![sample_chunk(
            "Oracle Database 19c",
            Some("Procedures"),
            "The procedure contains five major steps.",
        )],
        14,
    )
    .await
    .expect("synthesize");

    let request = received_request(&mock_server).await;
    let user_message = request["messages"][1]["content"]
        .as_str()
        .expect("user message");
    assert!(user_message.contains(custom_prompt));
}

#[tokio::test]
async fn test_synthesize_returns_chunks_only_when_llm_disabled() {
    let result = synthesize(
        None,
        false,
        "Summarize the procedure",
        "runsheet",
        None,
        vec![sample_chunk(
            "Oracle Database 19c",
            Some("Procedures"),
            "The procedure contains five major steps.",
        )],
        9,
    )
    .await
    .expect("synthesize");

    assert!(result.output.is_empty());
    assert!(result.synthesis_unavailable);
    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.chunks.len(), 1);
}

fn chat_client(mock_server: &MockServer) -> ChatClient {
    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = mock_server.uri();
    config.llm.librarian.model = "test-chat-model".to_string();
    config.llm.librarian.system_prompt = "You are a precise technical synthesizer.".to_string();
    ChatClient::new(&config).expect("chat client")
}

fn sample_chunk(book_title: &str, heading_path: Option<&str>, text: &str) -> SynthesisChunk {
    SynthesisChunk {
        chunk_id: format!("{book_title}-chunk"),
        book_id: format!("{book_title}-book"),
        book_title: book_title.to_string(),
        heading_path: heading_path.map(str::to_string),
        chunk_type: "procedure".to_string(),
        text: text.to_string(),
        word_count: text.split_whitespace().count() as i64,
        bm25_score: Some(0.91),
        cosine_score: Some(0.82),
        rrf_score: 1.0,
        rerank_score: Some(0.97),
    }
}

async fn received_request(mock_server: &MockServer) -> serde_json::Value {
    let requests = mock_server.received_requests().await.expect("received requests");
    let request = requests.first().expect("chat request");
    let body = String::from_utf8(request.body.clone()).expect("decode request body");
    serde_json::from_str(&body).expect("parse request body")
}

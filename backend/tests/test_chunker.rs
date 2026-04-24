#![allow(dead_code, unused_imports)]

use backend::ingest::chunker::{chunk_chapters, ChapterText, ChunkConfig, ChunkDomain, ChunkType};

fn chapter(text: &str) -> ChapterText {
    ChapterText {
        chapter_index: 0,
        title: String::new(),
        text: text.to_string(),
        is_image_heavy_page: false,
    }
}

#[test]
fn test_chunk_respects_target_size() {
    let text = (0..120)
        .map(|index| format!("word{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let chunks = chunk_chapters(
        &[chapter(&text)],
        &ChunkConfig {
            target_size: 50,
            overlap: 10,
            domain: ChunkDomain::Narrative,
        },
    );

    assert!(chunks.len() >= 3);
    assert!(chunks.iter().all(|chunk| chunk.word_count <= 50));
}

#[test]
fn test_chunk_never_splits_numbered_procedure() {
    let text = "\
1. Connect the cable
2. Turn the power on
3. Verify the status LED
4. Record the reading";
    let chunks = chunk_chapters(
        &[chapter(text)],
        &ChunkConfig {
            target_size: 20,
            overlap: 0,
            domain: ChunkDomain::Technical,
        },
    );

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].chunk_type, ChunkType::Procedure);
    assert!(chunks[0].text.contains("1. Connect the cable"));
    assert!(chunks[0].text.contains("4. Record the reading"));
}

#[test]
fn test_chunk_detects_heading_boundary() {
    let text = "\
# Alpha
First paragraph

## Beta
Second paragraph";
    let chunks = chunk_chapters(
        &[chapter(text)],
        &ChunkConfig {
            target_size: 50,
            overlap: 0,
            domain: ChunkDomain::Technical,
        },
    );

    assert!(chunks.len() >= 2);
    let first_index = chunks
        .iter()
        .position(|chunk| chunk.text.contains("First paragraph"))
        .expect("first paragraph chunk");
    let second_index = chunks
        .iter()
        .position(|chunk| chunk.text.contains("Second paragraph"))
        .expect("second paragraph chunk");
    assert!(first_index < second_index);
}

#[test]
fn test_chunk_heading_path_is_hierarchical() {
    let text = "\
# Admin Guide
## Part III
### §12.3
Body text";
    let chunks = chunk_chapters(
        &[chapter(text)],
        &ChunkConfig {
            target_size: 50,
            overlap: 0,
            domain: ChunkDomain::Technical,
        },
    );

    let body_chunk = chunks
        .iter()
        .find(|chunk| chunk.text.contains("Body text"))
        .expect("body chunk");
    assert_eq!(
        body_chunk.heading_path.as_deref(),
        Some("Admin Guide > Part III > §12.3")
    );
}

#[test]
fn test_chunk_marks_image_heavy_pages() {
    let chunk = chunk_chapters(
        &[ChapterText {
            chapter_index: 0,
            title: String::new(),
            text: "diagram labels only".to_string(),
            is_image_heavy_page: true,
        }],
        &ChunkConfig {
            target_size: 50,
            overlap: 0,
            domain: ChunkDomain::Technical,
        },
    )
    .into_iter()
    .next()
    .expect("chunk");

    assert!(chunk.is_image_heavy);
    assert_eq!(chunk.chunk_type, ChunkType::Image);
}

#[test]
fn test_culinary_domain_keeps_recipe_intact() {
    let text = "\
Simple Soup
Ingredients
Tomatoes
Water
Simmer until tender";
    let chunks = chunk_chapters(
        &[chapter(text)],
        &ChunkConfig {
            target_size: 80,
            overlap: 0,
            domain: ChunkDomain::Culinary,
        },
    );

    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].text.contains("Simple Soup"));
    assert!(chunks[0].text.contains("Simmer until tender"));
}

#[test]
fn test_overlap_tokens_appear_in_adjacent_chunks() {
    let text = (0..40)
        .map(|index| format!("token{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let chunks = chunk_chapters(
        &[chapter(&text)],
        &ChunkConfig {
            target_size: 12,
            overlap: 4,
            domain: ChunkDomain::Narrative,
        },
    );

    assert!(chunks.len() > 1);
    let first_words = chunks[0].text.split_whitespace().collect::<Vec<_>>();
    let second_words = chunks[1].text.split_whitespace().collect::<Vec<_>>();
    let overlap = 4.min(first_words.len()).min(second_words.len());
    assert_eq!(
        &first_words[first_words.len() - overlap..],
        &second_words[..overlap]
    );
}

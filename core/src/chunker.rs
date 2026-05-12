use crate::models::Chunk;
use crate::tokenizer::JapaneseTokenizer;

/// Split a Markdown document into chunks for indexing.
///
/// Currently this is a simple pass-through that treats the entire
/// document as a single chunk.  A future implementation will:
///  - split by heading hierarchy,
///  - respect token count limits for embedding models,
///  - populate `parent_header` with the closest ancestor heading path.
pub fn split_into_chunks(
    markdown: &str,
    tokenizer: &JapaneseTokenizer,
    file_path: &str,
) -> Vec<Chunk> {
    let tokenized = tokenizer.split(markdown);
    let chunk = Chunk {
        id: None,
        file_path: file_path.to_string(),
        chunk_index: 0,
        parent_header: None,
        content: markdown.to_string(),
        tokenized_content: tokenized,
    };
    vec![chunk]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::JapaneseTokenizer;

    #[test]
    fn test_single_chunk() {
        let md = "# Title\n\nSome content.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md");
        assert_eq!(chunks.len(), 1);
        let c = &chunks[0];
        assert_eq!(c.file_path, "test.md");
        assert_eq!(c.chunk_index, 0);
        assert!(c.parent_header.is_none());
        assert!(c.content.contains("Title"));
        assert!(c.tokenized_content.contains("Title"));
    }
}

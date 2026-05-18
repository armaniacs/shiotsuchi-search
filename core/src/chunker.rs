use crate::models::Chunk;
use crate::tokenizer::JapaneseTokenizer;

/// Maximum content length before attempting a paragraph-level split.
/// Sections longer than this are split on blank-line boundaries.
const LEVEL2_SPLIT_THRESHOLD: usize = 1000;

/// Split a Markdown document into chunks using recursive header/paragraph splitting.
///
/// Level 1: split on Markdown headers (`#`, `##`, `###`).
/// Level 2: if a section exceeds `LEVEL2_SPLIT_THRESHOLD` chars, split on blank-line
///          boundaries (paragraphs).  Each chunk receives `parent_header` set to the
///          ancestor heading path (e.g. `"Section 1 > Subsection A"`).
///
/// Fenced code blocks are never split internally.
pub fn split_into_chunks(
    markdown: &str,
    tokenizer: &JapaneseTokenizer,
    file_path: &str,
    vault_name: &str,
) -> Vec<Chunk> {
    if markdown.trim().is_empty() {
        return vec![Chunk {
            id: None,
            file_path: file_path.to_string(),
            chunk_index: 0,
            parent_header: None,
            content: String::new(),
            tokenized_content: String::new(),
            vault_name: vault_name.to_string(),
        }];
    }

    let sections = split_by_headers(markdown);
    let mut chunks: Vec<Chunk> = Vec::new();

    for (header_stack, section_text) in sections {
        let parent_header = if header_stack.is_empty() {
            None
        } else {
            Some(header_stack.join(" > "))
        };

        let trimmed = section_text.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.chars().count() > LEVEL2_SPLIT_THRESHOLD {
            // Level 2: split on blank lines (paragraph boundaries)
            let paragraphs = split_on_blank_lines(trimmed);
            for para in paragraphs {
                let para = para.trim();
                if para.is_empty() {
                    continue;
                }
                let idx = chunks.len() as i64;
                let tokenized = tokenizer.split(para);
                chunks.push(Chunk {
                    id: None,
                    file_path: file_path.to_string(),
                    chunk_index: idx,
                    parent_header: parent_header.clone(),
                    content: para.to_string(),
                    tokenized_content: tokenized,
                    vault_name: vault_name.to_string(),
                });
            }
        } else {
            let idx = chunks.len() as i64;
            let tokenized = tokenizer.split(trimmed);
            chunks.push(Chunk {
                id: None,
                file_path: file_path.to_string(),
                chunk_index: idx,
                parent_header: parent_header.clone(),
                content: trimmed.to_string(),
                tokenized_content: tokenized,
                vault_name: vault_name.to_string(),
            });
        }
    }

    // If the content had no splitting headers and no paragraphs, use whole doc
    if chunks.is_empty() {
        let tokenized = tokenizer.split(markdown.trim());
        chunks.push(Chunk {
            id: None,
            file_path: file_path.to_string(),
            chunk_index: 0,
            parent_header: None,
            content: markdown.trim().to_string(),
            tokenized_content: tokenized,
            vault_name: vault_name.to_string(),
        });
    }

    chunks
}

/// Split content on Markdown headers (`#`, `##`, `###`).
///
/// Returns `(header_stack, section_body)` for each section.  The first section
/// (before any header) has an empty stack.  Header levels deeper than 3 are
/// treated as regular lines and stay within the current section.
///
/// Fenced code blocks are tracked so that header-like markers inside them are
/// not treated as headings.
fn split_by_headers(content: &str) -> Vec<(Vec<String>, String)> {
    let mut sections: Vec<(Vec<String>, String)> = Vec::new();
    let mut current_headers: Vec<(usize, String)> = Vec::new();
    let mut current_body = String::new();
    let mut in_code_block = false;

    for line in content.lines() {
        // Track fenced code blocks
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            in_code_block = !in_code_block;
            current_body.push_str(line);
            current_body.push('\n');
            continue;
        }
        if in_code_block {
            current_body.push_str(line);
            current_body.push('\n');
            continue;
        }

        if let Some(level) = header_level(line) {
            // Flush current section before starting a new one
            if !current_body.trim().is_empty() || !current_headers.is_empty() {
                let stack: Vec<String> = current_headers.iter().map(|(_, t)| t.clone()).collect();
                sections.push((stack, std::mem::take(&mut current_body)));
            }
            // Pop headers at same or deeper level
            current_headers.retain(|(l, _)| *l < level);
            let title = line.trim_start_matches('#').trim().to_string();
            current_headers.push((level, title));
        } else {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }

    // Push final section
    let stack: Vec<String> = current_headers.iter().map(|(_, t)| t.clone()).collect();
    sections.push((stack, current_body));

    sections
}

/// Determine the heading level of a line (1-3 for `#`/`##`/`###`).
///
/// Returns `None` if the line is not a heading or uses a deeper level (`####`+),
/// which are treated as regular text.
fn header_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let count = trimmed.chars().take_while(|c| *c == '#').count();
    if count <= 3 && trimmed.as_bytes().get(count).copied() == Some(b' ') {
        Some(count)
    } else {
        None
    }
}

/// Split text on blank lines (paragraph boundaries), respecting fenced code blocks.
///
/// Consecutive blank lines are collapsed.  Whitespace-only lines are treated as
/// blank lines.  Blank lines inside fenced code blocks (``` or ~~~) are not
/// considered paragraph separators.
fn split_on_blank_lines(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_code_block = false;

    for line in text.lines() {
        // Detect fenced code block delimiter (allow leading whitespace)
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            in_code_block = !in_code_block;
            current.push_str(line);
            current.push('\n');
            continue;
        }

        if in_code_block {
            // Inside code block: accumulate without splitting on blank lines
            current.push_str(line);
            current.push('\n');
            continue;
        }

        // Outside code block
        if line.trim().is_empty() {
            if !current.trim().is_empty() {
                result.push(std::mem::take(&mut current));
            }
            // skip consecutive blank lines
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }

    if !current.trim().is_empty() {
        result.push(current);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_on_h1() {
        let md = "# Section 1\n\nContent A.\n\n# Section 2\n\nContent B.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].parent_header.as_deref(), Some("Section 1"));
        assert_eq!(chunks[1].parent_header.as_deref(), Some("Section 2"));
    }

    #[test]
    fn test_split_on_headers_h1_h2_h3() {
        let md = "# H1\n\nA\n\n## H2\n\nB\n\n### H3\n\nC";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].parent_header.as_deref(), Some("H1"));
        assert_eq!(chunks[1].parent_header.as_deref(), Some("H1 > H2"));
        assert_eq!(chunks[2].parent_header.as_deref(), Some("H1 > H2 > H3"));
    }

    #[test]
    fn test_header_popping() {
        let md = "## B\n\nX\n\n# A\n\nY";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        // First section under B (h2)
        assert_eq!(chunks[0].parent_header.as_deref(), Some("B"));
        // Then A (h1) should pop B
        assert_eq!(chunks[1].parent_header.as_deref(), Some("A"));
    }

    #[test]
    fn test_parent_header_hierarchy_is_correct() {
        // Empty sections (headers with no body) are skipped;
        // content is batched under the deepest non-empty header.
        let md = "# L1\n\n## L2\n\n### L3\n\nDeep content.\n\n## L2b\n\nShallow.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        // L1 and L2→L3 transitions produce empty sections → skipped
        // We get 2 chunks: one for L1 > L2 > L3, one for L1 > L2b
        assert_eq!(chunks.len(), 2, "expected 2, got {}", chunks.len());
        assert_eq!(chunks[0].parent_header.as_deref(), Some("L1 > L2 > L3"));
        assert!(chunks[0].content.contains("Deep content"));
        assert_eq!(chunks[1].parent_header.as_deref(), Some("L1 > L2b"));
        assert!(chunks[1].content.contains("Shallow"));
    }

    #[test]
    fn test_parent_header_populating_content_before_new_header() {
        // Content that appears before any header is top-level (no parent)
        let md = "Preamble.\n\n# H1\n\nBody.\n\n## H2\n\nDetail.\n\n# H3\n\nFinal.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        // 3 chunks: preamble (no parent), H1→H2, and H3
        // Actually: preamble is before H1, but H1 immediately follows it as a header.
        // Header H1 creates a section. Then H2 creates another. Then H3 creates another.
        // Empty sections between consecutive headers are skipped.
        // Preamble (top-level), H1 (with body), H1>H2 (with Detail), H3 (with Final)
        assert_eq!(chunks.len(), 4, "expected 4, got {}", chunks.len());
        assert!(chunks[0].parent_header.is_none());
        assert!(chunks[0].content.contains("Preamble"));
        assert_eq!(chunks[1].parent_header.as_deref(), Some("H1"));
        assert_eq!(chunks[2].parent_header.as_deref(), Some("H1 > H2"));
        assert_eq!(chunks[3].parent_header.as_deref(), Some("H3"));
    }

    // ── paragraph splitting (level 2) ────────────────────────────────

    #[test]
    fn test_long_section_splits_on_paragraphs() {
        // ~1200 chars with blank-line gaps → should cross the 1000-char threshold
        let md = "# Big Section\n\n".to_owned() + &"A\n\n".repeat(400) + "\n\nEnd.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(&md, &tok, "test.md", "default");
        assert!(chunks.len() > 1, "expected multiple chunks, got {}", chunks.len());
        for chunk in &chunks {
            assert_eq!(chunk.parent_header.as_deref(), Some("Big Section"));
        }
    }

    #[test]
    fn test_short_paragraphs_not_split() {
        let md = "# Small\n\nHello world.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        assert_eq!(chunks.len(), 1);
    }

    // ── code block awareness ─────────────────────────────────────────

    #[test]
    fn test_code_block_not_split() {
        let md = "# Code Demo\n\n```\n# This looks like a heading\n```\n\nTrailing text.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("# This looks like a heading"));
        assert!(chunks[0].content.contains("Trailing text"));
    }

    // ── edge cases ───────────────────────────────────────────────────

    #[test]
    fn test_no_headers_single_chunk() {
        let md = "Just text.\n\nNo headers.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].parent_header.is_none());
    }

    #[test]
    fn test_h4_ignored_as_header() {
        let md = "# H1\n\nX\n\n#### H4\n\nY";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        // H4 not a heading → everything under H1
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].parent_header.as_deref(), Some("H1"));
        assert!(chunks[0].content.contains("H4"));
    }

    #[test]
    fn test_empty_content_returns_single_chunk() {
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks("", &tok, "empty.md", "default");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.is_empty());
    }

    #[test]
    fn test_whitespace_only_returns_single_chunk() {
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks("   \n\n  ", &tok, "ws.md", "default");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.is_empty());
    }

    #[test]
    fn test_heading_without_space_not_treated_as_header() {
        let md = "#Not a heading\n\nContent.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("#Not"));
        assert!(chunks[0].parent_header.is_none());
    }

    #[test]
    fn test_chunks_have_correct_indices() {
        let md = "# A\n\nX\n\n# B\n\nY\n\n# C\n\nZ";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default");
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i as i64);
        }
    }

    #[test]
    fn test_split_on_blank_lines_basic() {
        let result = split_on_blank_lines("Para one.\n\nPara two.\n\nPara three.");
        assert_eq!(result.len(), 3);
        assert!(result[0].contains("Para one"));
    }

    #[test]
    fn test_split_on_blank_lines_no_split() {
        let result = split_on_blank_lines("Single paragraph.");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_frontmatter_only_content_returns_body_only() {
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let content = "---\ntitle: Test\n---";
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default");
        assert_eq!(chunks.len(), 1, "should still create 1 chunk");
        assert!(chunks[0].content.contains("title:"), "chunk should contain frontmatter key");
    }

    #[test]
    fn test_frontmatter_with_body_after() {
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let content = "---\ntitle: Test\n---\n\n# Actual content\n\nBody text here.";
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default");
        // The frontmatter block before the first heading creates a separate preamble chunk.
        // This is correct: the chunker treats --- separators as regular content.
        assert_eq!(chunks.len(), 2, "frontmatter + heading should create 2 chunks");
        assert!(chunks[0].content.contains("title:"), "first chunk should contain frontmatter");
        assert_eq!(chunks[1].parent_header.as_deref(), Some("Actual content"), "second chunk should be under 'Actual content' heading");
        assert!(chunks[1].content.contains("Body text"), "second chunk should contain body text");
    }

    #[test]
    fn test_h4_heading_does_not_split() {
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let content = "# Section 1\n\nContent.\n\n#### Subsection\n\nMore content.\n\n# Section 2\n\nFinal.";
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default");
        assert_eq!(chunks.len(), 2, "only h1 should split: h4 is not a split point");
    }

    #[test]
    fn test_h3_split_boundary() {
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let content = "# Top\n\nIntro.\n\n### Sub A\n\nContent A.\n\n### Sub B\n\nContent B.";
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default");
        assert!(chunks.len() >= 2, "h3 headers should create multiple chunks");
    }

    #[test]
    fn test_long_paragraph_splits_at_byte_threshold() {
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        // Create content that exceeds LEVEL2_SPLIT_THRESHOLD (1000 chars)
        // AND has blank-line paragraph boundaries.
        // Each paragraph is ~100 chars × 15 = ~1500 chars → well over threshold.
        let para = "This paragraph exceeds the byte threshold with repeated text that keeps going and going. ".repeat(15);
        let body = format!("{}{}{}", para, "\n\n", para);
        let content = format!("# Header\n\n{}", body);
        let chunks = split_into_chunks(&content, &tokenizer, "test.md", "default");
        // With two paragraphs each > 1000 chars (under the same heading),
        // should get at least 2 chunks from Level 2 splitting.
        assert!(chunks.len() >= 2, "long content should split into multiple chunks, got {}", chunks.len());
    }

    // ── header_level direct tests ────────────────────────────────────

    #[test]
    fn test_header_level_h1_to_h3() {
        assert_eq!(header_level("# Title"), Some(1));
        assert_eq!(header_level("## Subtitle"), Some(2));
        assert_eq!(header_level("### Subsubtitle"), Some(3));
    }

    #[test]
    fn test_header_level_h4_and_deeper_not_split_points() {
        assert_eq!(header_level("#### Deep"), None);
        assert_eq!(header_level("##### Deeper"), None);
        assert_eq!(header_level("###### Deepest"), None);
    }

    #[test]
    fn test_header_level_invalid_formats() {
        assert_eq!(header_level("#NoSpace"), None);
        assert_eq!(header_level("###"), None);
        assert_eq!(header_level("regular text"), None);
    }

    #[test]
    fn test_header_level_with_trailing_content() {
        assert_eq!(header_level("# Title | pipe"), Some(1));
        assert_eq!(header_level("## Code: `sample()`"), Some(2));
    }

    #[test]
    fn test_header_level_with_leading_whitespace() {
        assert_eq!(header_level("  # Indented"), Some(1));
        assert_eq!(header_level("\t## Tab"), Some(2));
    }

    #[test]
    fn test_header_level_unicode_title() {
        assert_eq!(header_level("# 日本語タイトル"), Some(1));
        assert_eq!(header_level("## 中文标题"), Some(2));
    }

    // ── split_by_headers direct tests ────────────────────────────────

    #[test]
    fn test_split_by_headers_header_in_code_block_not_split() {
        let md = "# Real Header\n\nContent.\n\n```\n# Fake Header\ncode\n```\n\nMore.";
        let sections = split_by_headers(md);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].1.contains("# Fake Header"));
    }

    #[test]
    fn test_split_by_headers_mixed_fence_types() {
        let md = "# Header\n\n```\ncode1\n~~~\nfake close\n```\n\nMore.";
        let sections = split_by_headers(md);
        assert_eq!(sections.len(), 1, "code block spanning multiple fence types");
    }

    #[test]
    fn test_split_by_headers_h1_then_h2_then_h3() {
        let md = "# H1\n\nA\n\n## H2\n\nB\n\n### H3\n\nC";
        let sections = split_by_headers(md);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].0, vec!["H1"]);
        assert_eq!(sections[1].0, vec!["H1", "H2"]);
        assert_eq!(sections[2].0, vec!["H1", "H2", "H3"]);
    }

    #[test]
    fn test_split_by_headers_header_level_pop() {
        let md = "# H1\n## H2a\nContent A\n## H2b\nContent B";
        let sections = split_by_headers(md);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[1].0, vec!["H1", "H2a"]);
        assert_eq!(sections[2].0, vec!["H1", "H2b"]);
    }

    #[test]
    fn test_split_by_headers_empty_sections() {
        let md = "# A\n# B\n# C";
        let sections = split_by_headers(md);
        assert!(sections.len() >= 2);
    }

    #[test]
    fn test_split_by_headers_unicode_headers() {
        let md = "# 日本語\n\n内容\n\n## 中文\n\n中文内容";
        let sections = split_by_headers(md);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].0, vec!["日本語"]);
        assert_eq!(sections[1].0, vec!["日本語", "中文"]);
    }

    // ── split_on_blank_lines edge cases ──────────────────────────────

    #[test]
    fn test_split_on_blank_lines_consecutive_blank_lines_collapsed() {
        let text = "Para 1\n\n\n\nPara 2";
        let paras = split_on_blank_lines(text);
        assert_eq!(paras.len(), 2, "consecutive blanks should be treated as one split");
    }

    #[test]
    fn test_split_on_blank_lines_whitespace_only_is_blank() {
        let text = "Para 1\n  \n\t\nPara 2";
        let paras = split_on_blank_lines(text);
        assert_eq!(paras.len(), 2, "whitespace-only lines are blank lines");
    }

    #[test]
    fn test_split_on_blank_lines_code_block_blank_lines_not_split() {
        // No blank line after closing fence — blank inside code block should not split
        let text = "Para 1\n\n```\ncode\n\nwith blank\n```\nPara 2";
        let paras = split_on_blank_lines(text);
        assert_eq!(paras.len(), 2);
        // First paragraph should NOT include code block content (separated by blank line)
        assert!(!paras[0].contains("code"), "code block should be in second paragraph");
        // Whole code block stays in one paragraph despite internal blank line
        assert!(paras[1].contains("code"));
        assert!(paras[1].contains("Para 2"));
    }

    #[test]
    fn test_split_on_blank_lines_tilde_fence() {
        let text = "Para 1\n~~~\ncode\n~~~\n\nPara 2";
        let paras = split_on_blank_lines(text);
        assert_eq!(paras.len(), 2);
    }

    #[test]
    fn test_split_on_blank_lines_indented_fence_markers() {
        let text = "Para 1\n  ```\ncode\n  ```\n\nPara 2";
        let paras = split_on_blank_lines(text);
        assert_eq!(paras.len(), 2);
    }

    #[test]
    fn test_split_on_blank_lines_empty_result() {
        let text = "   \n\n   ";
        let paras = split_on_blank_lines(text);
        assert_eq!(paras.len(), 0, "only blank lines yields empty result");
    }
}


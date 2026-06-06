use crate::frontmatter::extract_frontmatter;
use crate::models::Chunk;
use crate::tokenizer::{apply_user_dictionary_str, normalize, JapaneseTokenizer};

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
    user_dictionary: &[String],
) -> Vec<Chunk> {
    // 1. Extract and strip YAML frontmatter
    let (frontmatter, body) = extract_frontmatter(markdown);

    // Warn if any tag contains a comma — the CSV serialization will split
    // on commas downstream in reindex_file, fragmenting the tag.
    for tag in &frontmatter.tags {
        if tag.contains(',') {
            tracing::warn!(
                "Tag '{}' in '{}' contains a comma — will be split on reindex",
                tag, file_path
            );
        }
    }

    let markdown = body;
    if markdown.trim().is_empty() {
        return vec![Chunk {
            id: None,
            file_path: file_path.to_string(),
            chunk_index: 0,
            parent_header: None,
            content: String::new(),
            tokenized_content: String::new(),
            vault_name: vault_name.to_string(),
            tags: frontmatter.tags.join(","),
            frontmatter_date: frontmatter.date.unwrap_or_default(),
            title: frontmatter.title.unwrap_or_default(),
            emphasized_text: String::new(),
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
                let tokenized = tokenize_with_code_blocks(para, tokenizer, user_dictionary);
                chunks.push(Chunk {
                    id: None,
                    file_path: file_path.to_string(),
                    chunk_index: idx,
                    parent_header: parent_header.clone(),
                    content: para.to_string(),
                    tokenized_content: tokenized,
                    vault_name: vault_name.to_string(),
            tags: frontmatter.tags.join(","),
                    frontmatter_date: frontmatter.date.clone().unwrap_or_default(),
                    title: frontmatter.title.clone().unwrap_or_default(),
                    emphasized_text: String::new(),
                });
            }
        } else {
            let idx = chunks.len() as i64;
            let tokenized = tokenize_with_code_blocks(trimmed, tokenizer, user_dictionary);
            chunks.push(Chunk {
                id: None,
                file_path: file_path.to_string(),
                chunk_index: idx,
                parent_header: parent_header.clone(),
                content: trimmed.to_string(),
                tokenized_content: tokenized,
                vault_name: vault_name.to_string(),
                tags: frontmatter.tags.join(","),
                frontmatter_date: frontmatter.date.clone().unwrap_or_default(),
                title: frontmatter.title.clone().unwrap_or_default(),
                emphasized_text: String::new(),
            });
        }
    }

    // If the content had no splitting headers and no paragraphs, use whole doc
    if chunks.is_empty() {
        let tokenized = tokenize_with_code_blocks(markdown.trim(), tokenizer, user_dictionary);
        chunks.push(Chunk {
            id: None,
            file_path: file_path.to_string(),
            chunk_index: 0,
            parent_header: None,
            content: markdown.trim().to_string(),
            tokenized_content: tokenized,
            vault_name: vault_name.to_string(),
            tags: frontmatter.tags.join(","),
            frontmatter_date: frontmatter.date.unwrap_or_default(),
            title: frontmatter.title.unwrap_or_default(),
            emphasized_text: String::new(),
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

/// Segment content into alternating regular and code/math sections.
///
/// Returns `(text, is_code)` pairs where `is_code=true` for content inside:
/// - Fenced code blocks (`````` ``, `~~~`)
/// - Display math blocks (`$$...$$`)
/// - Inline code (`` ` ``)
/// - Inline math (`$...$`)
///
/// Regular text tokens are passed through Vaporetto; code/math tokens use simple
/// whitespace splitting.
fn split_code_math_segments(content: &str) -> Vec<(String, bool)> {
    let mut segments: Vec<(String, bool)> = Vec::new();
    let mut regular = String::new();
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let remaining: String = chars[i..].iter().map(|&(_, c)| c).collect();

        // Fenced code block (``` or ~~~) — must be at start of line
        let is_fence = (remaining.starts_with("```") || remaining.starts_with("~~~"))
            && (i == 0 || chars[i - 1].1 == '\n');
        if is_fence {
            if !regular.is_empty() {
                split_inline_segments(&mut segments, regular.clone());
                regular.clear();
            }
            let fence_char = chars[i].1;
            let mut j = i + 3;
            // skip rest of opening line
            while j < chars.len() && chars[j].1 != '\n' {
                j += 1;
            }
            if j < chars.len() {
                j += 1; // skip newline
            }
            // look for closing fence (same char, 3+ times, at start of line)
            let mut found = false;
            while j + 2 < chars.len() {
                if chars[j].1 == fence_char
                    && chars[j + 1].1 == fence_char
                    && chars[j + 2].1 == fence_char
                    && (j == i + 3 || chars[j - 1].1 == '\n')
                {
                    let mut k = j + 3;
                    while k < chars.len() && chars[k].1 != '\n' {
                        k += 1;
                    }
                    segments.push((content[chars[i].0..chars[k].0].to_string(), true));
                    i = k;
                    if i < chars.len() && chars[i].1 == '\n' {
                        i += 1;
                    }
                    found = true;
                    break;
                }
                j += 1;
            }
            if !found {
                segments.push((content[chars[i].0..].to_string(), true));
                break;
            }
            continue;
        }

        // Display math block ($$...$$)
        if chars[i].1 == '$' && i + 1 < chars.len() && chars[i + 1].1 == '$' {
            if !regular.is_empty() {
                split_inline_segments(&mut segments, regular.clone());
                regular.clear();
            }
            let mut j = i + 2;
            while j + 1 < chars.len() {
                if chars[j].1 == '$' && chars[j + 1].1 == '$' {
                    segments.push((content[chars[i].0..chars[j].0 + 2].to_string(), true));
                    i = j + 2;
                    break;
                }
                j += 1;
            }
            if j + 1 >= chars.len() {
                segments.push((content[chars[i].0..].to_string(), true));
                break;
            }
            continue;
        }

        regular.push(chars[i].1);
        i += 1;
    }

    if !regular.is_empty() {
        split_inline_segments(&mut segments, regular);
    }

    segments
}

/// Process regular text for inline code (`` ` ``) and inline math (`$...$`),
/// splitting them into separate segments.
fn split_inline_segments(segments: &mut Vec<(String, bool)>, text: String) {
    // Collect (byte_offset, char) pairs so we can slice `text` correctly
    // when multi-byte UTF-8 characters are present.
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    let mut start = 0;

    while i < chars.len() {
        // Inline code: backtick (not part of ```)
        if chars[i].1 == '`' && !(i + 2 < chars.len() && chars[i + 1].1 == '`' && chars[i + 2].1 == '`')
        {
            let mut j = i + 1;
            while j < chars.len() && chars[j].1 != '`' && chars[j].1 != '\n' {
                j += 1;
            }
            if j < chars.len() && chars[j].1 == '`' {
                if i > start {
                    segments.push((text[chars[start].0..chars[i].0].to_string(), false));
                }
                segments.push((text[chars[i].0..chars[j].0 + 1].to_string(), true));
                start = j + 1;
                i = j + 1;
                continue;
            }
        }

        // Inline math: $ (not part of $$)
        if chars[i].1 == '$' && !(i + 1 < chars.len() && chars[i + 1].1 == '$') {
            let mut j = i + 1;
            while j < chars.len() && chars[j].1 != '$' && chars[j].1 != '\n' {
                j += 1;
            }
            if j < chars.len() && chars[j].1 == '$' {
                if i > start {
                    segments.push((text[chars[start].0..chars[i].0].to_string(), false));
                }
                segments.push((text[chars[i].0..chars[j].0 + 1].to_string(), true));
                start = j + 1;
                i = j + 1;
                continue;
            }
        }

        i += 1;
    }

    if start < chars.len() {
        segments.push((text[chars[start].0..].to_string(), false));
    }
}

/// Tokenize content with code/math-aware segmentation.
/// Code and math blocks use whitespace splitting; regular text uses Vaporetto.
fn tokenize_with_code_blocks(
    content: &str,
    tokenizer: &JapaneseTokenizer,
    user_dictionary: &[String],
) -> String {
    let segments = split_code_math_segments(content);
    let mut tokenized: Vec<String> = Vec::new();
    for (text, is_code) in &segments {
        let tok = tokenizer.tokenize_content(text, *is_code);
        if !tok.is_empty() {
            tokenized.push(tok);
        }
    }
    let combined = tokenized.join(" ");
    normalize(&apply_user_dictionary_str(&combined, user_dictionary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_on_h1() {
        let md = "# Section 1\n\nContent A.\n\n# Section 2\n\nContent B.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].parent_header.as_deref(), Some("Section 1"));
        assert_eq!(chunks[1].parent_header.as_deref(), Some("Section 2"));
    }

    #[test]
    fn test_split_on_headers_h1_h2_h3() {
        let md = "# H1\n\nA\n\n## H2\n\nB\n\n### H3\n\nC";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].parent_header.as_deref(), Some("H1"));
        assert_eq!(chunks[1].parent_header.as_deref(), Some("H1 > H2"));
        assert_eq!(chunks[2].parent_header.as_deref(), Some("H1 > H2 > H3"));
    }

    #[test]
    fn test_header_popping() {
        let md = "## B\n\nX\n\n# A\n\nY";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
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
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
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
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
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
        let chunks = split_into_chunks(&md, &tok, "test.md", "default", &[]);
        assert!(chunks.len() > 1, "expected multiple chunks, got {}", chunks.len());
        for chunk in &chunks {
            assert_eq!(chunk.parent_header.as_deref(), Some("Big Section"));
        }
    }

    #[test]
    fn test_short_paragraphs_not_split() {
        let md = "# Small\n\nHello world.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
    }

    // ── code block awareness ─────────────────────────────────────────

    #[test]
    fn test_code_block_not_split() {
        let md = "# Code Demo\n\n```\n# This looks like a heading\n```\n\nTrailing text.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("# This looks like a heading"));
        assert!(chunks[0].content.contains("Trailing text"));
    }

    // ── edge cases ───────────────────────────────────────────────────

    #[test]
    fn test_no_headers_single_chunk() {
        let md = "Just text.\n\nNo headers.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].parent_header.is_none());
    }

    #[test]
    fn test_h4_ignored_as_header() {
        let md = "# H1\n\nX\n\n#### H4\n\nY";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        // H4 not a heading → everything under H1
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].parent_header.as_deref(), Some("H1"));
        assert!(chunks[0].content.contains("H4"));
    }

    #[test]
    fn test_empty_content_returns_single_chunk() {
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks("", &tok, "empty.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.is_empty());
    }

    #[test]
    fn test_whitespace_only_returns_single_chunk() {
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks("   \n\n  ", &tok, "ws.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.is_empty());
    }

    #[test]
    fn test_heading_without_space_not_treated_as_header() {
        let md = "#Not a heading\n\nContent.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("#Not"));
        assert!(chunks[0].parent_header.is_none());
    }

    #[test]
    fn test_chunks_have_correct_indices() {
        let md = "# A\n\nX\n\n# B\n\nY\n\n# C\n\nZ";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
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
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default", &[]);
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
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default", &[]);
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
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 2, "only h1 should split: h4 is not a split point");
    }

    #[test]
    fn test_h3_split_boundary() {
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let content = "# Top\n\nIntro.\n\n### Sub A\n\nContent A.\n\n### Sub B\n\nContent B.";
        let chunks = split_into_chunks(content, &tokenizer, "test.md", "default", &[]);
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
        let chunks = split_into_chunks(&content, &tokenizer, "test.md", "default", &[]);
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

    #[test]
    fn test_unclosed_code_fence_at_eof_does_not_panic() {
        // A code block fence that is never closed should not cause a panic
        // or leave the chunker in an inconsistent state.
        let md = "# Header\n\nContent.\n\n```\nunclosed code block";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1, "should produce one chunk for the whole doc");
        assert!(chunks[0].content.contains("unclosed code block"));
    }

    #[test]
    fn test_tilde_fence_unclosed_at_eof() {
        let md = "# Tilde Fence\n\nContent.\n\n~~~\ncode block never closed";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("code block never closed"));
    }

    #[test]
    fn test_nested_fence_markers_not_confused() {
        // Closing fence type different from opening fence should not close it
        let md = "# Nesting\n\n```\nopened with backticks\n~~~\nnot a real close\n```\n\nNow closed.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        // With correct fence tracking, the section after the fence should
        // be in the same chunk as the code block (since headers inside fences are not split).
        assert_eq!(chunks.len(), 1, "code block should not be split by inner ~~~");
    }

    #[test]
    fn test_code_block_opened_backtick_closed_tilde_mixed() {
        let md = "# Mixed Fence\n\n```\ncode block\n~~~\nstill inside\n```\n\nOutside.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1,
            "``` opened with ~~~ close should remain one chunk (block not closed)");
        assert!(chunks[0].content.contains("Outside."),
            "content after attempted close should be in same chunk");
    }

    #[test]
    fn test_code_block_backtick_open_tilde_as_content() {
        let md = "Paragraph.\n\n```\nregular ~~~ fence\n```\n\nOutside.";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1,
            "~~~ inside ``` block should not close it");
        assert!(chunks[0].content.contains("Outside."),
            "content after block should be inside same chunk");
    }

// ── split_code_math_segments UTF-8 safety ──────────────────────

#[test]
fn test_code_fence_after_multibyte_text_does_not_panic() {
    let content = "日本語の説明\n```\ncode block\n```\n";
    let segments = split_code_math_segments(content);
    // prefix + fenced code + trailing newline
    assert!(segments.len() >= 2, "should have at least prefix + code");
    assert!(!segments[0].1, "prefix before fence should be regular");
    let code_seg = segments.iter().find(|(_, is_code)| *is_code);
    assert!(code_seg.is_some(), "fenced code should be marked as code");
    assert!(code_seg.unwrap().0.contains("code block"));
}

#[test]
fn test_display_math_after_multibyte_text_does_not_panic() {
    let content = "日本語の説明$$\na + b\n$$\n";
    let segments = split_code_math_segments(content);
    let math_seg = segments.iter().find(|(_, is_code)| *is_code);
    assert!(math_seg.is_some(), "math block should be marked as code/math");
    assert!(math_seg.unwrap().0.contains("a + b"));
}

#[test]
fn test_tilde_fence_after_multibyte_text() {
    let content = "日本語\n~~~\ncode\n~~~\n";
    let segments = split_code_math_segments(content);
    let code_seg = segments.iter().find(|(_, is_code)| *is_code);
    assert!(code_seg.is_some(), "tilde fence should be code");
}

#[test]
fn test_multibyte_unclosed_fence_does_not_panic() {
    let content = "日本語\n```\nnever closed";
    let segments = split_code_math_segments(content);
    // prefix + unclosed code block
    assert_eq!(segments.len(), 2, "prefix + unclosed code block");
    assert!(!segments[0].1, "prefix should be regular");
    let code_seg = segments.iter().find(|(_, is_code)| *is_code);
    assert!(code_seg.is_some(), "unclosed fence content should be code");
}

// ── normalize tests ──────────────────────────────────────────

#[test]
fn test_normalize_fullwidth_in_tokenized_content() {
        let md = "# Fullwidth\n\nＡＢＣテスト";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        // Tokenized content should be normalized: fullwidth ASCII → halfwidth lowercase
        let tok = &chunks[0].tokenized_content;
        assert!(
            tok.contains("abc"),
            "tokenized content should contain normalized 'abc', got: {}",
            tok
        );
        // Original (non-normalized) form should NOT appear
        assert!(
            !tok.contains('Ａ'),
            "fullwidth chars should not appear in tokenized content: {}",
            tok
        );
    }

    #[test]
    fn test_normalize_mixed_case_in_tokenized_content() {
        let md = "# Mixed Case\n\nHello World";
        let tok = crate::require_tokenizer!(Default::default());
        let chunks = split_into_chunks(md, &tok, "test.md", "default", &[]);
        assert_eq!(chunks.len(), 1);
        let tok = &chunks[0].tokenized_content;
        assert!(
            tok.contains("hello"),
            "tokenized content should contain lowercase 'hello', got: {}",
            tok
        );
        assert!(
            tok.contains("world"),
            "tokenized content should contain lowercase 'world', got: {}",
            tok
        );
        assert!(
            !tok.contains("Hello"),
            "original case should not appear in tokenized content: {}",
            tok
        );
    }
}


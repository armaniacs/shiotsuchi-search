/// Lightweight YAML frontmatter parser for Markdown notes.
///
/// Extracts `title`, `tags`, and `date` fields from `---`-delimited blocks.
/// Handles both inline arrays (`tags: [a, b]`) and multi-line lists
/// (`tags:\n  - a\n  - b`). All other YAML keys are silently ignored.

/// Parsed frontmatter metadata.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub date: Option<String>,
}

/// Extract frontmatter from Markdown content and return the body (without frontmatter).
///
/// If no `---` delimiter is found at the start, returns `(Frontmatter::default(), content)`.
/// The frontmatter block is stripped from the returned body string.
pub fn extract_frontmatter(content: &str) -> (Frontmatter, &str) {
    let content = content.trim_start();

    if !content.starts_with("---") {
        return (Frontmatter::default(), content);
    }

    // Check that the opening `---` is at the start of a line (first line or after newline)
    // Since we trimmed_start, content starts at the beginning.
    let after_opener = &content[3..];
    let after_opener = after_opener.trim_start();

    // Find the closing `---` or `...`
    let end = find_closing_delimiter(after_opener);
    let end = match end {
        Some(pos) => pos,
        None => return (Frontmatter::default(), content), // No closing delimiter
    };

    let raw_frontmatter = &after_opener[..end];

    // Skip the rest after closing delimiter (trimmed)
    let body_start = end + 3; // skip past `---`
    let rest = &after_opener[body_start..];
    let body = rest.trim_start();

    let fm = parse_frontmatter_lines(raw_frontmatter);

    (fm, body)
}

/// Find the closing `---` or `...` delimiter position.
fn find_closing_delimiter(content: &str) -> Option<usize> {
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let mut pos = 0usize;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            return Some(pos);
        }
        pos += line.len();
    }

    None
}

/// Parse key-value lines from the raw frontmatter block.
fn parse_frontmatter_lines(content: &str) -> Frontmatter {
    let mut title = None;
    let mut tags: Vec<String> = Vec::new();
    let mut date = None;
    let mut in_tags_list = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            in_tags_list = false;
            continue;
        }

        // Check for multi-line tag list continuation
        if in_tags_list {
            if let Some(item) = trimmed.strip_prefix("- ") {
                let tag = item.trim().to_string();
                if !tag.is_empty() {
                    tags.push(tag);
                }
                continue;
            }
            // If the line doesn't start with "- ", we're done with the tag list
            in_tags_list = false;
        }

        // Try "key: value" pattern
        let colon_pos = match trimmed.find(':') {
            Some(p) => p,
            None => continue,
        };

        let key = trimmed[..colon_pos].trim().to_lowercase();
        let value = trimmed[colon_pos + 1..].trim();

        match key.as_str() {
            "title" => {
                if !value.is_empty() {
                    title = Some(value.to_string());
                }
            }
            "date" => {
                if !value.is_empty() {
                    date = Some(value.to_string());
                }
            }
            "tags" => {
                if value.is_empty() {
                    // tags key with no value on same line → start of multi-line list
                    in_tags_list = true;
                } else if value.starts_with('[') && value.ends_with(']') {
                    // Inline array: [tag1, tag2, tag3]
                    let inner = &value[1..value.len() - 1];
                    for item in inner.split(',') {
                        let item = item.trim().trim_matches('"').trim_matches('\'');
                        if !item.is_empty() {
                            tags.push(item.to_string());
                        }
                    }
                } else {
                    // Comma-separated string: tag1, tag2
                    for item in value.split(',') {
                        let item = item.trim().trim_matches('"').trim_matches('\'');
                        if !item.is_empty() {
                            tags.push(item.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Frontmatter { title, tags, date }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_frontmatter() {
        let content = "# Hello\n\nBody text.";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm, Frontmatter::default());
        assert_eq!(body, content);
    }

    #[test]
    fn test_basic_frontmatter() {
        let content = "---\ntitle: My Note\ntags: [project, meeting]\ndate: 2026-01-15\n---\n\n# Body";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm.title.as_deref(), Some("My Note"));
        assert_eq!(fm.tags, vec!["project", "meeting"]);
        assert_eq!(fm.date.as_deref(), Some("2026-01-15"));
        assert_eq!(body.trim(), "# Body");
    }

    #[test]
    fn test_frontmatter_missing_closing_delimiter_falls_back() {
        let content = "---\ntitle: Broken\nno closing delimiter";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm, Frontmatter::default());
        assert_eq!(body, content);
    }

    #[test]
    fn test_frontmatter_no_frontmatter_at_all() {
        let content = "# Just a heading\n\nSome content.";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm, Frontmatter::default());
        assert_eq!(body, content);
    }

    #[test]
    fn test_frontmatter_with_multi_line_tags() {
        let content = "---\ntitle: Multi Tags\ntags:\n  - rust\n  - cli\n  - search\ndate: 2026-03-01\n---\n\nContent.";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm.title.as_deref(), Some("Multi Tags"));
        assert_eq!(fm.tags, vec!["rust", "cli", "search"]);
        assert_eq!(fm.date.as_deref(), Some("2026-03-01"));
    }

    #[test]
    fn test_frontmatter_comma_separated_tags() {
        let content = "---\ntags: rust, cli, search\ndate: 2026-04-01\n---\n\nContent.";
        let (fm, _body) = extract_frontmatter(content);
        assert_eq!(fm.tags, vec!["rust", "cli", "search"]);
    }

    #[test]
    fn test_frontmatter_no_tags() {
        let content = "---\ndate: 2026-05-01\n---\n\nContent.";
        let (fm, _body) = extract_frontmatter(content);
        assert!(fm.tags.is_empty());
        assert_eq!(fm.date.as_deref(), Some("2026-05-01"));
    }

    #[test]
    fn test_frontmatter_no_date() {
        let content = "---\ntitle: No Date\ntags: [test]\n---\n\nContent.";
        let (fm, _body) = extract_frontmatter(content);
        assert_eq!(fm.title.as_deref(), Some("No Date"));
        assert!(fm.date.is_none());
    }

    #[test]
    fn test_frontmatter_unknown_keys_ignored() {
        let content = "---\nalias: old-name\ncssclass: dashboard\n---\n\nBody.";
        let (fm, _body) = extract_frontmatter(content);
        assert_eq!(fm, Frontmatter::default());
    }

    #[test]
    fn test_frontmatter_with_trailing_spaces() {
        let content = "---  \ntitle: Padded  \n---  \n\nBody.";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm.title.as_deref(), Some("Padded"));
        assert_eq!(body.trim(), "Body.");
    }

    #[test]
    fn test_frontmatter_empty_tags() {
        let content = "---\ntags: []\n---\n\nBody.";
        let (fm, _body) = extract_frontmatter(content);
        assert!(fm.tags.is_empty());
    }

    #[test]
    fn test_frontmatter_whitespace_only_body_ok() {
        let content = "---\ntitle: Whitespace Only\n---\n  \n\n  ";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm.title.as_deref(), Some("Whitespace Only"));
        assert!(body.trim().is_empty());
    }
}

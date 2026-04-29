use std::path::Path;

/// Extract YAML frontmatter from markdown content.
/// Returns (title, body_without_frontmatter).
/// If no frontmatter, returns (None, original_content).
pub fn extract_frontmatter(content: &str) -> (Option<String>, String) {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return (None, content.to_string());
    }

    let end_marker = "\n---\n";
    let end_marker_crlf = "\r\n---\r\n";

    if let Some(end_pos) = content.find(end_marker) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker.len()..];
        let title = parse_yaml_title(frontmatter);
        return (title, body.to_string());
    }

    if let Some(end_pos) = content.find(end_marker_crlf) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker_crlf.len()..];
        let title = parse_yaml_title(frontmatter);
        return (title, body.to_string());
    }

    (None, content.to_string())
}

fn parse_yaml_title(frontmatter: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix("title:") {
            let value = stripped.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Parse markdown to plain text.
pub fn markdown_to_text(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser};

    let parser = Parser::new(markdown);
    let mut text = String::new();
    for event in parser {
        match event {
            Event::Text(t) => text.push_str(&t),
            Event::Code(c) => text.push_str(&c),
            Event::HardBreak | Event::SoftBreak => text.push('\n'),
            _ => {}
        }
    }
    // Collapse multiple newlines
    text.lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate title from filename stem.
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .replace('-', " ")
        .replace('_', " ")
}

#[cfg(test)]
mod parsing_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_no_frontmatter() {
        let content = "# Hello\n\nWorld";
        let (title, body) = extract_frontmatter(content);
        assert!(title.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_with_frontmatter() {
        let content = "---\ntitle: My Note\ntags: [a, b]\n---\n\n# Body\nText";
        let (title, body) = extract_frontmatter(content);
        assert_eq!(title, Some("My Note".to_string()));
        assert!(body.contains("Body"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn test_markdown_to_text() {
        let md = "# Title\n\n**Bold** text and `code`.\n\n- item1\n- item2";
        let text = markdown_to_text(md);
        assert!(text.contains("Bold"));
        assert!(text.contains("code"));
        assert!(!text.contains("#"));
        assert!(!text.contains("**"));
    }

    #[test]
    fn test_title_from_path() {
        assert_eq!(title_from_path(&PathBuf::from("my-note.md")), "my note");
        assert_eq!(title_from_path(&PathBuf::from("dir/file_name.md")), "file name");
    }
}

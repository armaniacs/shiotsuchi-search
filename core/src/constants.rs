/// Maximum number of characters in a search result snippet.
/// When the extracted snippet exceeds this length, it is truncated with "…".
pub const MAX_SNIPPET_CHARS: usize = 500;

/// Fallback snippet length used when no query token is found in the text.
/// This provides a reasonable preview even when the search term does not appear verbatim.
pub const FALLBACK_SNIPPET_CHARS: usize = 200;

/// Default number of context lines before and after the matched line in a snippet.
/// The total snippet spans (2 * DEFAULT_SNIPPET_LINES + 1) lines.
pub const DEFAULT_SNIPPET_LINES: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_values() {
        assert_eq!(MAX_SNIPPET_CHARS, 500);
        assert_eq!(FALLBACK_SNIPPET_CHARS, 200);
        assert_eq!(DEFAULT_SNIPPET_LINES, 3);
    }
}

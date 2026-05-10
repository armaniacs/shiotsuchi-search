/// Default number of context lines before and after the matched line in a snippet.
/// The total snippet spans (2 * DEFAULT_SNIPPET_LINES + 1) lines.
pub const DEFAULT_SNIPPET_LINES: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_values() {
        assert_eq!(DEFAULT_SNIPPET_LINES, 3);
    }
}

/// Default number of context lines before and after the matched line in a snippet.
/// The total snippet spans (2 * DEFAULT_SNIPPET_LINES + 1) lines.
pub const DEFAULT_SNIPPET_LINES: usize = 3;

/// Expected SHA-256 hex digest of the official `model.onnx` file.
///
/// To compute: `sha256sum ~/.local/share/shiotsuchi/model.onnx`
///
/// Set to `""` to skip hash verification (e.g. during development with a
/// custom model).  When empty, `setup --check` will still detect the file
/// but will not verify its checksum.
pub const EXPECTED_MODEL_SHA256: &str = "";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_values() {
        assert_eq!(DEFAULT_SNIPPET_LINES, 3);
        // EXPECTED_MODEL_SHA256 may be empty (skip verification) or a 64-char hex string.
        if !EXPECTED_MODEL_SHA256.is_empty() {
            assert_eq!(EXPECTED_MODEL_SHA256.len(), 64, "SHA-256 hex must be 64 characters");
        }
    }
}

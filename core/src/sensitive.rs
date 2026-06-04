//! Sensitive data detection and masking.
//!
//! This module provides functionality to detect and mask sensitive data in text
//! before returning it to untrusted clients (MCP, HTTP API). CLI output is not
//! masked as it's considered a trusted environment.
//!
//! # Design Decisions
//! - Masking is applied ONLY on output, NOT on stored data
//! - Default: disabled (opt-in via configuration)
//! - Patterns are derived from TruffleHog detectors (regex only, no verification)

use regex::Regex;

use crate::sensitive_patterns::{get_builtin_patterns, get_placeholder};

/// Configuration for sensitive data detection and masking.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SensitiveDataConfig {
    /// Whether to enable sensitive data masking
    pub detection: bool,
    /// Custom regex patterns to add (space-separated placeholders will be auto-generated)
    pub patterns: Vec<String>,
}

impl SensitiveDataConfig {
    /// Check if sensitive data detection is enabled
    pub fn is_enabled(&self) -> bool {
        self.detection
    }
}

/// Mask sensitive data in text based on configuration.
///
/// Returns the original text unchanged if detection is disabled or config is None.
/// Otherwise, replaces detected sensitive patterns with placeholders like [EMAIL], [API_KEY], etc.
///
/// # Arguments
/// * `text` - The text to potentially mask
/// * `config` - Optional configuration; if None or disabled, returns text unchanged
///
/// # Performance Considerations
/// - Regex patterns are compiled once and cached via LazyLock
/// - For large documents, this is O(n * m) where n is text length and m is number of patterns
pub fn mask_sensitive_data(text: &str, config: Option<&SensitiveDataConfig>) -> String {
    let config = match config {
        Some(c) if c.is_enabled() => c,
        _ => return text.to_string(),
    };

    let mut result = text.to_string();

    // Apply built-in patterns
    let regexes = get_builtin_regexes();
    for (i, re) in regexes.iter().enumerate() {
        let placeholder = get_placeholder(i);
        result = re.replace_all(&result, format!("[{}]", placeholder)).to_string();
    }

    // Apply custom patterns
    for pattern in &config.patterns {
        if let Ok(re) = Regex::new(pattern) {
            result = re.replace_all(&result, "[CUSTOM_SECRET]").to_string();
        }
    }

    result
}

/// Lazy-initialized compiled regexes for built-in patterns.
/// This avoids recompiling patterns on every call.
fn get_builtin_regexes() -> &'static [Regex] {
    static REGEXES: std::sync::OnceLock<Vec<Regex>> = std::sync::OnceLock::new();
    REGEXES.get_or_init(|| {
        get_builtin_patterns()
            .into_iter()
            .map(|p| {
                Regex::new(p).unwrap_or_else(|e| {
                    panic!("Invalid built-in sensitive pattern: {} — error: {}", p, e)
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(enabled: bool, patterns: Vec<String>) -> SensitiveDataConfig {
        SensitiveDataConfig {
            detection: enabled,
            patterns,
        }
    }

    #[test]
    fn test_mask_disabled_does_nothing() {
        let text = "Contact user@example.com or use sk-12345678901234567890123456789012345678901234567890";
        let config = make_config(false, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert_eq!(masked, text);
    }

    #[test]
    fn test_mask_none_returns_unchanged() {
        let text = "user@example.com";
        let masked = mask_sensitive_data(text, None);
        assert_eq!(masked, text);
    }

    #[test]
    fn test_mask_email_addresses() {
        let text = "Contact user@example.com for details";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert_eq!(masked, "Contact [EMAIL] for details");
    }

    #[test]
    fn test_mask_multiple_emails() {
        let text = "Email alice@example.com or bob@test.org";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[EMAIL]"));
        assert!(!masked.contains("example.com"));
        assert!(!masked.contains("test.org"));
    }

    #[test]
    fn test_mask_api_keys() {
        let text = "API_KEY=sk-12345678901234567890123456789012345678901234567890";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[OPENAI_KEY]"));
        assert!(!masked.contains("1234567890"));
    }

    #[test]
    fn test_mask_anthropic_key() {
        let text = "ANTHROPIC_API_KEY=sk-ant-1234567890123456789012345678901234567890";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[ANTHROPIC_KEY]"));
    }

    #[test]
    fn test_mask_github_token() {
        let text = "GITHUB_TOKEN=ghp_1234567890123456789012345678901234567";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[GITHUB_TOKEN]"));
    }

    #[test]
    fn test_mask_gitlab_token() {
        let text = "GITLAB_TOKEN=glpat-1234567890123456789012";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[GITLAB_TOKEN]"));
    }

    #[test]
    fn test_mask_phone_numbers() {
        let text = "Call +1-555-123-4567 or 555-999-8888";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[PHONE]"));
    }

    #[test]
    fn test_mask_custom_pattern() {
        let text = "Customer ID: CUST-123456";
        let config = make_config(true, vec!["CUST-\\d{6}".to_string()]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[CUSTOM_SECRET]"));
        assert!(!masked.contains("CUST-123456"));
    }

    #[test]
    fn test_mask_multiple_patterns_in_one_text() {
        let text = "Email user@example.com with API key sk-12345678901234567890123456789012345678901234567890 and phone +1-555-123-4567";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[EMAIL]"));
        assert!(masked.contains("[OPENAI_KEY]"));
        assert!(masked.contains("[PHONE]"));
        assert!(!masked.contains("user@example.com"));
    }

    #[test]
    fn test_mask_jwt_token() {
        let text = "Token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adIf8q5c";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[JWT_TOKEN]"));
        assert!(!masked.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn test_mask_invalid_custom_pattern_ignored() {
        let text = "Some text here";
        let config = make_config(true, vec!["[invalid(regex".to_string()]);
        // Should not panic, just ignore invalid pattern
        let masked = mask_sensitive_data(text, Some(&config));
        assert_eq!(masked, text);
    }

    #[test]
    fn test_mask_preserves_surrounding_text() {
        let text = "The secret key is: sk-12345678901234567890123456789012345678901234567890 and that's it.";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.starts_with("The secret key is: [OPENAI_KEY]"));
        assert!(masked.ends_with("and that's it."));
    }

    #[test]
    fn test_mask_japanese_text_with_sensitive_data() {
        let text = "連絡先: user@example.com です。また、APIキー: sk-12345678901234567890123456789012345678901234567890";
        let config = make_config(true, vec![]);
        let masked = mask_sensitive_data(text, Some(&config));
        assert!(masked.contains("[EMAIL]"));
        assert!(masked.contains("[OPENAI_KEY]"));
        assert!(!masked.contains("user@example.com"));
    }
}
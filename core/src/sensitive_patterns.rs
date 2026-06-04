//! Sensitive data detection patterns.
//!
//! These patterns are derived from TruffleHog detectors and other well-known
//! secret detection patterns. Only regex patterns are included; dynamic
//! verification is intentionally excluded.
//!
//! Reference: https://github.com/trufflesecurity/trufflehog

use std::sync::OnceLock;

/// Built-in sensitive data patterns with their mask replacements.
/// Each tuple contains: (regex pattern, placeholder name)
fn get_builtin_patterns_inner() -> Vec<(&'static str, &'static str)> {
    vec![
        // PII - Email addresses
        (r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}", "EMAIL"),
        // OpenAI API keys (sk-... format, typically 48+ chars)
        (r"sk-[a-zA-Z0-9]{48,}", "OPENAI_KEY"),
        // Anthropic API keys (sk-ant-... format)
        (r"sk-ant-[a-zA-Z0-9\-]{40,}", "ANTHROPIC_KEY"),
        // GitHub Personal Access Tokens (ghp_, ghrs_, ghu_, ghs_, ght_ prefixes)
        (r"gh[puso]_[a-zA-Z0-9]{36,}", "GITHUB_TOKEN"),
        // GitLab Personal Access Tokens (glpat- prefix)
        (r"glpat-[a-zA-Z0-9\-]{20,}", "GITLAB_TOKEN"),
        // AWS Access Key ID (AKIA... or ASIA... prefix)
        (r"AKIA[0-9A-Z]{16}", "AWS_KEY_ID"),
        // AWS Secret Access Key — requires explicit AWS context to avoid false positives
        // on random 40-char strings (UUIDs, hashes, etc.)
        (r"(?i)aws.{0,20}[A-Za-z0-9/+=]{40}", "AWS_SECRET"),
        // Slack tokens (xox[baprs]-... format)
        (r"xox[baprs]-[a-zA-Z0-9-]+", "SLACK_TOKEN"),
        // Discord tokens
        (r"mfa\.[a-zA-Z0-9_\-]{84}|[a-zA-Z0-9_\-]{24}\.[a-zA-Z0-9_\-]{6}\.[a-zA-Z0-9_\-]{27}", "DISCORD_TOKEN"),
        // Google API Keys (GCP)
        (r"AIza[a-zA-Z0-9_\-]{35}", "GCP_API_KEY"),
        // Azure connection strings
        (r"DefaultEndpointsProtocol=https;AccountName=[a-zA-Z0-9]+;AccountKey=[a-zA-Z0-9]+(?:==)?;", "AZURE_CONN_STR"),
        // URL embedded credentials (https://user:password@...)
        (r"(?:https?://)(?:[^:]+):([^@]+)@", "URL_CREDENTIAL"),
        // JWT tokens
        (r"eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+", "JWT_TOKEN"),
        // Private keys (PEM format)
        // Uses (?s) flag via inline modifier to make . match newlines efficiently
        // and avoid ReDoS from [\s\S]*? backtracking on large documents.
        (r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----(?s:.+)-----END (?:RSA |EC |DSA )?PRIVATE KEY-----", "PRIVATE_KEY"),
        // PII - Phone numbers (must come after API keys to avoid false matches)
        // Requires at least one separator or a '+' prefix to avoid matching bare digit sequences in API keys
        (r"\+[\d\-\.\s\(\)]{7,20}|\d{3,4}[-.\s]\d{3,4}[-.\s]\d{4}", "PHONE"),
    ]
}

/// Cached built-in sensitive data patterns.
pub fn get_builtin_patterns() -> Vec<(&'static str, &'static str)> {
    static PATTERNS: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(get_builtin_patterns_inner).clone()
}

/// Get the placeholder name for a pattern index
pub fn get_placeholder(patterns: &[(&'static str, &'static str)], index: usize) -> &'static str {
    patterns.get(index).map(|(_, placeholder)| *placeholder).unwrap_or("REDACTED")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patterns_compile() {
        for (pattern, _) in get_builtin_patterns() {
            let re = regex::Regex::new(pattern);
            assert!(
                re.is_ok(),
                "Pattern '{}' should compile successfully",
                pattern
            );
        }
    }

    #[test]
    fn test_email_pattern_matches() {
        let pattern = get_builtin_patterns()[0].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("user@example.com"));
        assert!(re.is_match("test.user@domain.co.jp"));
        assert!(!re.is_match("not an email"));
    }

    #[test]
    fn test_openai_key_pattern() {
        let pattern = get_builtin_patterns()[1].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("sk-12345678901234567890123456789012345678901234567890"));
        assert!(!re.is_match("sk-short"));
    }

    #[test]
    fn test_anthropic_key_pattern() {
        let pattern = get_builtin_patterns()[2].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("sk-ant-1234567890123456789012345678901234567890"));
    }

    #[test]
    fn test_github_token_pattern() {
        let pattern = get_builtin_patterns()[3].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("ghp_1234567890123456789012345678901234567"));
        assert!(re.is_match("gho_12345678901234567890123456789012345678"));
        assert!(re.is_match("ghs_12345678901234567890123456789012345678"));
    }

    #[test]
    fn test_gitlab_token_pattern() {
        let pattern = get_builtin_patterns()[4].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("glpat-1234567890123456789012"));
    }

    #[test]
    fn test_aws_key_id_pattern() {
        let pattern = get_builtin_patterns()[5].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("AKIAIOSFODNN7EXAMPLE"));
        assert!(re.is_match("AKIA0123456789ABCDEF"));
    }

    #[test]
    fn test_slack_token_pattern() {
        let pattern = get_builtin_patterns()[7].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("xoxb-1234567890-1234567890123-abc123def456"));
        assert!(re.is_match("xoxa-1234567890-1234567890123-abc123def456"));
    }

    #[test]
    fn test_gcp_api_key_pattern() {
        let pattern = get_builtin_patterns()[9].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("AIzaSy1234567890123456789012345678901234567"));
    }

    #[test]
    fn test_jwt_pattern() {
        let pattern = get_builtin_patterns()[12].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match(
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adIf8q5c"
        ));
    }

    #[test]
    fn test_phone_pattern() {
        let pattern = get_builtin_patterns()[14].0;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("+1-555-123-4567"));
        assert!(re.is_match("555-123-4567"));
        assert!(re.is_match("+81 90-1234-5678"));
        // Bare digit sequences (like those in API keys) should NOT match
        assert!(!re.is_match("123456789012345678901234567890"), "bare digits should not match phone pattern");
    }
}

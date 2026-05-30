// VLM (Vision Language Model) based PDF extraction for scanned PDFs.
// Uses edgequake-pdf2md to render PDF pages to images and convert to Markdown.
//
// This module is gated by the `vlm` feature flag. When disabled, all functions
// return empty results (no-op).

use std::path::Path;
use std::sync::OnceLock;

use crate::config::VlmConfig;

#[derive(Debug, thiserror::Error)]
pub enum VlmError {
    #[error("VLM feature not compiled (add --features vlm)")]
    NotCompiled,
    #[error("VLM extraction disabled (set [vlm] enabled = true in config)")]
    Disabled,
    #[error("API key not found: set SHIOTSUCHI_API_KEY or {0}_API_KEY env var")]
    MissingApiKey(String),
    #[error("conversion failed: {0}")]
    ConversionFailed(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Global tokio runtime reused across all VLM calls to avoid per-call thread pool setup.
/// Returns an error if tokio runtime creation fails, preserving the original error-recovery
/// path so the caller can log a warning and continue gracefully instead of panicking.
fn tokio_runtime() -> Result<&'static tokio::runtime::Runtime, VlmError> {
    static RT: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Runtime::new()
            .map_err(|e| format!("failed to create tokio runtime for VLM: {}", e))
    })
    .as_ref()
    .map_err(|msg| VlmError::ConversionFailed(msg.clone()))
}

/// Extract text from a PDF using VLM, returning Markdown.
/// Returns Ok(Some(text)) on success, Ok(None) if VLM is not configured/enabled,
/// or Err if something goes wrong.
#[cfg(feature = "vlm")]
pub fn extract_text_with_vlm(
    file_path: &Path,
    vlm_config: &VlmConfig,
) -> Result<Option<String>, VlmError> {
    if !vlm_config.enabled {
        return Ok(None);
    }

    // Check for API key
    let api_key = std::env::var("SHIOTSUCHI_API_KEY").ok();
    if api_key.is_none() {
        // Also check provider-specific env vars
        let provider_upper = vlm_config.provider.to_uppercase();
        let provider_key_var = format!("{}_API_KEY", provider_upper);
        if std::env::var(&provider_key_var).is_err() {
            return Err(VlmError::MissingApiKey(vlm_config.provider.clone()));
        }
    }

    // Build ConversionConfig
    let mut builder = edgequake_pdf2md::ConversionConfig::builder()
        .provider_name(&vlm_config.provider)
        .model(&vlm_config.model);

    if let Some(max_pages) = vlm_config.max_pages_per_doc {
        builder = builder.pages(edgequake_pdf2md::PageSelection::Range(1, max_pages));
    }

    let config = builder
        .build()
        .map_err(|e| VlmError::ConversionFailed(e.to_string()))?;

    // Read PDF bytes
    let bytes = std::fs::read(file_path)
        .map_err(|e| VlmError::Io(e.to_string()))?;

    // Reuse a global tokio runtime instead of creating one per call
    let rt = tokio_runtime()?;

    let result = rt.block_on(async {
        edgequake_pdf2md::convert_from_bytes(&bytes, &config).await
    });

    match result {
        Ok(output) => {
            let text = output.markdown.trim().to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        Err(e) => Err(VlmError::ConversionFailed(e.to_string())),
    }
}

/// Stub when vlm feature is disabled.
#[cfg(not(feature = "vlm"))]
pub fn extract_text_with_vlm(
    _file_path: &Path,
    _vlm_config: &VlmConfig,
) -> Result<Option<String>, VlmError> {
    Err(VlmError::NotCompiled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vlm_config_default_disabled() {
        let config = VlmConfig::default();
        assert!(!config.enabled, "VLM should be disabled by default");
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4.1-nano");
        assert!(config.max_pages_per_doc.is_none());
    }

    #[test]
    fn test_extract_disabled_returns_none() {
        let config = VlmConfig {
            enabled: false,
            ..Default::default()
        };
        let path = Path::new("/nonexistent/test.pdf");
        let result = extract_text_with_vlm(path, &config);
        match result {
            Ok(None) => {} // expected: disabled = Ok(None)
            Ok(Some(_)) => panic!("should not return text when disabled"),
            Err(e) => panic!("disabled should return Ok(None), got Err({})", e),
        }
    }

    #[test]
    fn test_extract_missing_api_key_returns_error() {
        // Temporarily clear relevant env vars
        let old_shiotsuchi = std::env::var_os("SHIOTSUCHI_API_KEY");
        std::env::remove_var("SHIOTSUCHI_API_KEY");
        let old_openai = std::env::var_os("OPENAI_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");

        let config = VlmConfig {
            enabled: true,
            provider: "openai".to_string(),
            ..Default::default()
        };
        let path = Path::new("/nonexistent/test.pdf");
        let result = extract_text_with_vlm(path, &config);
        assert!(result.is_err(), "should error when API key is missing");
        if let Err(VlmError::MissingApiKey(provider)) = &result {
            assert_eq!(provider, "openai", "should mention provider name");
        } else {
            panic!("expected MissingApiKey error, got: {:?}", result);
        }

        // Restore env vars
        if let Some(val) = old_shiotsuchi {
            std::env::set_var("SHIOTSUCHI_API_KEY", val);
        }
        if let Some(val) = old_openai {
            std::env::set_var("OPENAI_API_KEY", val);
        }
    }
}

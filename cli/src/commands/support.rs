//! `support` subcommand — build and runtime information.

use crate::config::ShiotsuchiConfig;
use clap::Args;
use shiotsuchi_core::build_info::HAS_MODEL_EMBEDDED;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct SupportArgs {
    #[arg(long)]
    pub json: bool,
}

pub fn run_support(
    args: &SupportArgs,
    cfg: &ShiotsuchiConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let info = BuildInfo::gather(cfg)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        info.print_table();
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct BuildInfo {
    build: BuildFeatures,
    dependencies: DependencyFeatures,
    runtime: RuntimeInfo,
    config: ConfigSnapshot,
}

#[derive(Debug, serde::Serialize)]
struct BuildFeatures {
    watcher: bool,
    async_index: bool,
    model_embedded: bool,
    model_hash: String,
}

#[derive(Debug, serde::Serialize)]
struct DependencyFeatures {
    ort_download_binaries: bool,
    vaporetto_charwise_pma: bool,
    vaporetto_tag_prediction: bool,
    vaporetto_cache_type_score: bool,
    vaporetto_fix_weight_length: bool,
    rusqlite_bundled: bool,
}

#[derive(Debug, serde::Serialize)]
struct RuntimeInfo {
    notes_dir: PathBuf,
    db_path: PathBuf,
    model_path: Option<PathBuf>,
    model_hash: Option<String>,
    model_hash_verified: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
struct ConfigSnapshot {
    indexing: IndexingConfigSnapshot,
    watcher: WatcherConfigSnapshot,
}

#[derive(Debug, serde::Serialize)]
struct IndexingConfigSnapshot {
    snippet_lines: usize,
    max_snippet_chars: usize,
    include_extensions: Vec<String>,
    exclude_dirs: Vec<String>,
    auto_exclude_hidden: bool,
    follow_links: bool,
    dynamic_threshold: usize,
}

#[derive(Debug, serde::Serialize)]
struct WatcherConfigSnapshot {
    enabled: bool,
}

impl BuildInfo {
    fn gather(cfg: &ShiotsuchiConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let model_path = shiotsuchi_core::embedder::resolve_model_path(None);
        let (model_hash, model_hash_verified) = if let Some(ref path) = model_path {
            match shiotsuchi_core::embedder::verify_model_hash(path) {
                Ok(verified) => (hash_of(path).ok(), Some(verified)),
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };

        let vaults = cfg.resolved_vaults();
        let primary_notes_dir = vaults
            .first()
            .map(|(_, d)| d.clone())
            .unwrap_or_default();
        let db_path = cfg.resolved_db_path();

        Ok(BuildInfo {
            build: BuildFeatures {
                watcher: shiotsuchi_core::build_info::FEATURE_WATCHER,
                async_index: shiotsuchi_core::build_info::FEATURE_ASYNC_INDEX,
                model_embedded: HAS_MODEL_EMBEDDED,
                model_hash: shiotsuchi_core::build_info::EMBEDDED_MODEL_HASH.into(),
            },
            dependencies: DependencyFeatures {
                ort_download_binaries: shiotsuchi_core::build_info::DEP_ORT_DOWNLOAD_BINARIES,
                vaporetto_charwise_pma: shiotsuchi_core::build_info::DEP_VAPORETTO_CHARWISE_PMA,
                vaporetto_tag_prediction: shiotsuchi_core::build_info::DEP_VAPORETTO_TAG_PREDICTION,
                vaporetto_cache_type_score:
                    shiotsuchi_core::build_info::DEP_VAPORETTO_CACHE_TYPE_SCORE,
                vaporetto_fix_weight_length:
                    shiotsuchi_core::build_info::DEP_VAPORETTO_FIX_WEIGHT_LENGTH,
                rusqlite_bundled: shiotsuchi_core::build_info::DEP_RUSQLITE_BUNDLED,
            },
            runtime: RuntimeInfo {
                notes_dir: primary_notes_dir,
                db_path,
                model_path,
                model_hash,
                model_hash_verified,
            },
            config: ConfigSnapshot {
                indexing: IndexingConfigSnapshot {
                    snippet_lines: cfg.indexing.snippet_lines,
                    max_snippet_chars: cfg.indexing.max_snippet_chars,
                    include_extensions: cfg.indexing.include_extensions.clone(),
                    exclude_dirs: cfg.indexing.exclude_dirs.clone(),
                    auto_exclude_hidden: cfg.indexing.auto_exclude_hidden,
                    follow_links: cfg.indexing.follow_links,
                    dynamic_threshold: cfg.indexing.dynamic_threshold,
                },
                watcher: WatcherConfigSnapshot {
                    enabled: cfg.watcher.enabled,
                },
            },
        })
    }

    fn print_table(&self) {
        println!("=== Build Features ===");
        println!("  watcher:       {}", self.build.watcher);
        println!("  async-index:   {}", self.build.async_index);
        println!("  model-embedded:{}", self.build.model_embedded);
        if !self.build.model_hash.is_empty() {
            println!("  model-hash:    {}", self.build.model_hash);
        }

        println!();
        println!("=== Dependency Features ===");
        println!(
            "  ort-download-binaries:      {}",
            self.dependencies.ort_download_binaries
        );
        println!(
            "  vaporetto-charwise-pma:     {}",
            self.dependencies.vaporetto_charwise_pma
        );
        println!(
            "  vaporetto-tag-prediction:   {}",
            self.dependencies.vaporetto_tag_prediction
        );
        println!(
            "  vaporetto-cache-type-score: {}",
            self.dependencies.vaporetto_cache_type_score
        );
        println!(
            "  vaporetto-fix-weight-length:{}",
            self.dependencies.vaporetto_fix_weight_length
        );
        println!(
            "  rusqlite-bundled:           {}",
            self.dependencies.rusqlite_bundled
        );

        println!();
        println!("=== Runtime ===");
        println!("  notes-dir: {}", self.runtime.notes_dir.display());
        println!("  db-path:   {}", self.runtime.db_path.display());
        match &self.runtime.model_path {
            Some(p) => println!("  model-path: {}", p.display()),
            None => println!("  model-path: (not found)"),
        }
        match &self.runtime.model_hash {
            Some(h) => println!("  model-hash: {}", h),
            None => println!("  model-hash: (unavailable)"),
        }
        match self.runtime.model_hash_verified {
            Some(true) => println!("  model-hash-verified: yes"),
            Some(false) => println!("  model-hash-verified: no (mismatch)"),
            None => println!("  model-hash-verified: (skipped)"),
        }

        println!();
        println!("=== Config ===");
        println!(
            "  snippet-lines:        {}",
            self.config.indexing.snippet_lines
        );
        println!(
            "  max-snippet-chars:    {}",
            self.config.indexing.max_snippet_chars
        );
        println!(
            "  include-extensions:   {:?}",
            self.config.indexing.include_extensions
        );
        println!(
            "  exclude-dirs:         {:?}",
            self.config.indexing.exclude_dirs
        );
        println!(
            "  auto-exclude-hidden:  {}",
            self.config.indexing.auto_exclude_hidden
        );
        println!(
            "  follow-links:         {}",
            self.config.indexing.follow_links
        );
        println!(
            "  dynamic-threshold:    {}",
            self.config.indexing.dynamic_threshold
        );
        println!("  watcher-enabled:      {}", self.config.watcher.enabled);
    }
}

fn hash_of(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShiotsuchiConfig;

    #[test]
    fn gather_includes_config_values() {
        let cfg = ShiotsuchiConfig::default();
        let info = BuildInfo::gather(&cfg).unwrap();
        assert_eq!(
            info.config.indexing.include_extensions,
            vec!["md", "markdown"]
        );
    }

    #[test]
    fn json_output_contains_all_top_level_keys() {
        let cfg = ShiotsuchiConfig::default();
        let info = BuildInfo::gather(&cfg).unwrap();
        let json = serde_json::to_value(&info).unwrap();
        assert!(json.get("build").is_some());
        assert!(json.get("dependencies").is_some());
        assert!(json.get("runtime").is_some());
        assert!(json.get("config").is_some());
    }
}

use crate::config::IndexingConfig;
use clap::Args;
use shiotsuchi_core::{
    db::NoteDatabase,
    embedder::{resolve_model_path, Embedder},
    indexer::{index_directory, IndexResult},
    models::IndexConfig,
    tokenizer::get_tokenizer,
};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct ChartArgs {
    /// Deprecated: use `shiotsuchi init --force` instead.
    #[arg(long, hide = true)]
    pub force: bool,
    #[arg(long)]
    pub quiet: bool,
}

pub struct ChartSummary {
    pub indexed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub invalid_patterns: usize,
}

pub fn run_chart(
    args: &ChartArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
) -> Result<ChartSummary, Box<dyn std::error::Error>> {
    if args.force {
        eprintln!("warning: --force is deprecated and has no effect on chart; use `shiotsuchi init --force` instead");
    }
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::util::secure_parent_dir(db_path);
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };

    let embedder = resolve_model_path(None).and_then(|p| match Embedder::load(&p) {
        Ok(e) => {
            if !args.quiet {
                eprintln!("[info] Embedder model loaded — vector indexing enabled.");
            }
            Some(e)
        }
        Err(e) => {
            if !args.quiet {
                eprintln!("[warn] Could not load embedder: {}.", e);
            }
            None
        }
    });

    let (results, invalid_patterns) = index_directory(&db, &tokenizer, &config, embedder.as_ref(), None)?;

    let mut summary = ChartSummary {
        indexed: 0,
        skipped: 0,
        errors: 0,
        invalid_patterns,
    };
    for (_, _, result) in &results {
        match result {
            IndexResult::Inserted | IndexResult::Updated => summary.indexed += 1,
            IndexResult::Skipped => summary.skipped += 1,
            IndexResult::Error(_) => summary.errors += 1,
        }
    }

    if embedder.is_none() && !args.quiet {
        eprintln!(
            "[info] Embedder model not found — vector indexing skipped. \
             Run `shiotsuchi setup` to enable semantic search."
        );
    }

    if !args.quiet {
        let mut msg = format!(
            "Indexed {} files ({} skipped, {} errors)",
            summary.indexed, summary.skipped, summary.errors
        );
        if summary.invalid_patterns > 0 {
            msg.push_str(&format!(
                ", {} invalid pattern{}",
                summary.invalid_patterns,
                if summary.invalid_patterns == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        println!("{}", msg);
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IndexingConfig;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_chart_indexes_files() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("note.md"), "# Hello\n\nWorld").unwrap();

        let db_file = temp.path().join("test.db");
        let args = ChartArgs {
            force: false,
            quiet: true,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_chart(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg);
        match result {
            Ok(summary) => {
                assert_eq!(summary.indexed, 1);
                assert_eq!(summary.errors, 0);
            }
            Err(e) => {
                // If the model is unavailable, `get_tokenizer()` returns NoModel.
                let msg = format!("{}", e);
                if msg.contains("no model") || msg.contains("NoModel") {
                    eprintln!("[SKIPPED] chart::test_chart_indexes_files — Vaporetto model not available (set SHIOTSUCHI_MODEL_PATH)");
                    return;
                }
                panic!("chart test failed: {}", e);
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_chart_creates_parent_dir_with_0700() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("note.md"), "# Test").unwrap();

        let db_path = temp.path().join("cache").join("subdir").join("test.db");
        let args = ChartArgs {
            force: false,
            quiet: true,
        };
        let idx_cfg = IndexingConfig::default();
        let _result = run_chart(&args, &[("default".to_string(), vault.clone())], &db_path, &idx_cfg);

        let parent = db_path.parent().unwrap();
        if parent.exists() {
            let mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o700,
                "parent directory should have 0o700 permissions"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_chart_parent_dir_0700_with_nested_path() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let vault = temp.path();
        fs::write(vault.join("note.md"), "# Test").unwrap();

        let db_path = temp.path().join("a").join("b").join("c").join("test.db");
        let args = ChartArgs {
            force: false,
            quiet: true,
        };
        let idx_cfg = IndexingConfig::default();
        let _result = run_chart(&args, &[("default".to_string(), vault.to_path_buf())], &db_path, &idx_cfg);

        // secure_parent_dir sets 0o700 on the immediate parent (c)
        // ancestor directories (a, b) are not modified by the utility
        let immediate_parent = db_path.parent().unwrap();
        if immediate_parent.exists() {
            let mode = std::fs::metadata(immediate_parent)
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode, 0o700,
                "immediate parent should have 0o700 permissions"
            );
        }
    }
}

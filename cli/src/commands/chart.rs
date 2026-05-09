use crate::config::IndexingConfig;
use clap::Args;
use shiotsuchi_core::{
    db::NoteDatabase,
    indexer::index_directory,
    models::{IndexConfig, IndexResult},
    tokenizer::get_tokenizer,
};
use std::path::Path;

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
    notes_dir: &Path,
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
) -> Result<ChartSummary, Box<dyn std::error::Error>> {
    if args.force {
        eprintln!("warning: --force is deprecated and has no effect on chart; use `shiotsuchi init --force` instead");
    }
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(parent) {
                if meta.permissions().mode() & 0o777 != 0o700 {
                    if let Err(e) =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                    {
                        log::warn!("Failed to set parent directory permissions to 0o700: {}", e);
                    }
                }
            }
        }
    }
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        notes_dir: notes_dir.to_path_buf(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };

    let (results, invalid_patterns) = index_directory(&db, &tokenizer, &config)?;

    let mut summary = ChartSummary {
        indexed: 0,
        skipped: 0,
        errors: 0,
        invalid_patterns,
    };
    for (_, result) in &results {
        match result {
            IndexResult::Inserted | IndexResult::Updated => summary.indexed += 1,
            IndexResult::Skipped => summary.skipped += 1,
            IndexResult::Error(_) => summary.errors += 1,
        }
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
        let result = run_chart(&args, temp.path(), &db_file, &idx_cfg);
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
}

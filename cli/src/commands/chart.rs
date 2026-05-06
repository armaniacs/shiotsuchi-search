use clap::Args;
use crate::config::IndexingConfig;
use shiotsuchi_core::{
    db::NoteDatabase,
    indexer::index_directory,
    models::{IndexConfig, IndexResult},
    tokenizer::get_tokenizer,
};
use std::path::Path;

#[derive(Args, Debug)]
pub struct ChartArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub quiet: bool,
}

pub struct ChartSummary {
    pub indexed: usize,
    pub skipped: usize,
    pub errors: usize,
}

pub fn run_chart(
    args: &ChartArgs,
    notes_dir: &Path,
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
) -> Result<ChartSummary, Box<dyn std::error::Error>> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        notes_dir: notes_dir.to_path_buf(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_patterns: indexing_cfg.exclude_patterns.clone(),
    };

    let results = index_directory(&db, &tokenizer, &config)?;

    let mut summary = ChartSummary {
        indexed: 0,
        skipped: 0,
        errors: 0,
    };
    for (_, result) in &results {
        match result {
            IndexResult::Inserted | IndexResult::Updated => summary.indexed += 1,
            IndexResult::Skipped => summary.skipped += 1,
            IndexResult::Error(_) => summary.errors += 1,
        }
    }

    if !args.quiet {
        println!(
            "Indexed {} files ({} skipped, {} errors)",
            summary.indexed, summary.skipped, summary.errors
        );
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

use clap::Args;
use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    indexer::index_directory,
    models::{IndexConfig, IndexResult},
    tokenizer::{JapaneseTokenizer, TokenizerConfig},
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
) -> Result<ChartSummary, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())?;
    let config = IndexConfig {
        notes_dir: notes_dir.to_path_buf(),
        ..Default::default()
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
        let result = run_chart(&args, temp.path(), &db_file);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.indexed, 1);
        assert_eq!(summary.errors, 0);
    }
}

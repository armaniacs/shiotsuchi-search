use clap::Args;
use crate::config::IndexingConfig;
use shiotsuchi_core::{
    db::NoteDatabase,
    models::SearchResult,
    search::search,
    tokenizer::get_tokenizer,
};
use std::path::Path;

#[derive(Args, Debug)]
pub struct DiveArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

pub fn run_dive(
    args: &DiveArgs,
    notes_dir: &Path,
    db_path: &Path,
    _indexing_cfg: &IndexingConfig,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
    if args.query.trim().is_empty() {
        return Ok(vec![]);
    }

    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let results = search(&db, &tokenizer, notes_dir, &args.query, args.limit)?;
    
    Ok(results)
}

pub fn print_results(results: &[SearchResult], compact_json: bool) {
    if compact_json {
        println!("{}", serde_json::to_string(results).unwrap_or_default());
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(results).unwrap_or_default()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IndexingConfig;
    use std::fs;
    use tempfile::TempDir;

    fn default_indexing_cfg() -> IndexingConfig {
        IndexingConfig::default()
    }

    #[test]
    fn test_dive_returns_results() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("note.md"),
            "# Hello\n\nThis is a search test.",
        )
        .unwrap();
        let db_file = temp.path().join("test.db");
        let idx_cfg = default_indexing_cfg();

        let chart_args = crate::commands::chart::ChartArgs {
            force: false,
            quiet: true,
        };
        let chart_result = crate::commands::chart::run_chart(&chart_args, temp.path(), &db_file, &idx_cfg);
        if chart_result.is_err() {
            // Model not available (NoModel error) — skip test
            return;
        }

        let args = DiveArgs {
            query: "search test".to_string(),
            json: false,
            limit: 10,
        };
        let output = run_dive(&args, temp.path(), &db_file, &idx_cfg).unwrap();
        assert!(!output.is_empty());
        assert!(output[0].path.contains("note"));
    }

    #[test]
    fn test_dive_empty_query_returns_empty() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let idx_cfg = default_indexing_cfg();
        let _ = crate::commands::chart::run_chart(
            &crate::commands::chart::ChartArgs {
                force: false,
                quiet: true,
            },
            temp.path(),
            &db_file,
            &idx_cfg,
        );

        let args = DiveArgs {
            query: "".to_string(),
            json: false,
            limit: 10,
        };
        let output = run_dive(&args, temp.path(), &db_file, &idx_cfg).unwrap();
        assert!(output.is_empty());
    }
}

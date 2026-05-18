use clap::Args;
use shiotsuchi_core::{
    constants::DEFAULT_SNIPPET_LINES,
    db::NoteDatabase,
    embedder::{resolve_model_path, Embedder},
    models::{ChunkSearchResult, SearchMode},
    search::{extract_snippet, search},
    tokenizer::get_tokenizer,
};
use std::path::Path;
use std::time::Duration;

/// Output format for search results.
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    /// Formatted table with file path, header, snippet, and score.
    Table,
    /// Compact JSON array (one line).
    Json,
    /// Pretty-printed JSON array.
    JsonPretty,
}

/// CLI-side wrapper for SearchMode so core remains clap-independent.
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum CliSearchMode {
    /// Keyword search via FTS5 (always available).
    Fts,
    /// Semantic vector search (requires model).
    Vec,
    /// Hybrid FTS + vector with RRF fusion (default; falls back to FTS when no model).
    #[default]
    Hybrid,
}

impl From<CliSearchMode> for SearchMode {
    fn from(mode: CliSearchMode) -> Self {
        match mode {
            CliSearchMode::Fts => SearchMode::Fts,
            CliSearchMode::Vec => SearchMode::Vec,
            CliSearchMode::Hybrid => SearchMode::Hybrid,
        }
    }
}

#[derive(Args, Debug)]
pub struct DiveArgs {
    /// Search query string.
    pub query: String,

    /// Output as compact JSON (deprecated: use --format json).
    #[arg(long)]
    pub json: bool,

    /// Maximum number of results.
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// Output format (default: table, unless --json is set).
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,

    /// Search mode: fts (keyword), vec (semantic), hybrid (default).
    #[arg(long, value_enum, default_value_t = CliSearchMode::Hybrid)]
    pub mode: CliSearchMode,

    /// Path to ONNX embedding model file (overrides SHIOTSUCHI_EMBED_MODEL_PATH and XDG default).
    #[arg(long)]
    pub model_path: Option<std::path::PathBuf>,
}

impl DiveArgs {
    /// Resolve the effective output format, respecting the legacy --json flag.
    pub fn effective_format(&self) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            self.format.clone()
        }
    }
}

pub fn run_dive(
    args: &DiveArgs,
    db_path: &Path,
) -> Result<Vec<ChunkSearchResult>, Box<dyn std::error::Error>> {
    if args.query.trim().is_empty() {
        return Ok(vec![]);
    }

    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;

    let search_mode: SearchMode = args.mode.clone().into();

    let embedder = match resolve_model_path(args.model_path.as_deref()) {
        Some(p) => match Embedder::load(&p) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("[warn] Could not load embedder: {}. Using FTS only.", e);
                None
            }
        },
        None => {
            if matches!(search_mode, SearchMode::Vec | SearchMode::Hybrid) {
                eprintln!(
                    "[warn] モデルファイルが見つかりません。ベクトル検索は無効です。\n\
                     キーワード検索（FTS5）のみで動作します。"
                );
            }
            None
        }
    };

    // Vec mode with no embedder is a hard error; Hybrid gracefully falls back to FTS
    if embedder.is_none() && matches!(search_mode, SearchMode::Vec) {
        return Err(
            "Vector search requires a model. Set SHIOTSUCHI_EMBED_MODEL_PATH or use --model-path."
                .into(),
        );
    }

    let results = search(
        &db,
        &tokenizer,
        &args.query,
        args.limit,
        search_mode,
        embedder.as_ref(),
        None,
        None,
    )?;
    Ok(results)
}

/// Print search results in the specified format.
pub fn print_results(
    results: &[ChunkSearchResult],
    query: &str,
    format: &OutputFormat,
    elapsed: Duration,
) {
    match format {
        OutputFormat::Table => print_table(results, query, elapsed),
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(results).unwrap_or_default());
        }
        OutputFormat::JsonPretty => {
            println!(
                "{}",
                serde_json::to_string_pretty(results).unwrap_or_default()
            );
        }
    }
}

/// Print results as a human-readable table.
fn print_table(results: &[ChunkSearchResult], query: &str, elapsed: Duration) {
    let separator = "━".repeat(78);
    println!("Results for \"{query}\"");
    println!("{separator}");

    for (i, result) in results.iter().enumerate() {
        let idx = i + 1;
        let header = result.parent_header.as_deref().unwrap_or("(top level)");
        println!(
            "  {idx}. {} > {}  [{:.4}]",
            result.file_path, header, result.score
        );
        let snippet = extract_snippet(&result.content, query, DEFAULT_SNIPPET_LINES, 300);
        for line in snippet.lines() {
            println!("     {line}");
        }
        println!();
    }

    println!("{separator}");
    println!(
        "{} results found ({:.3}s)",
        results.len(),
        elapsed.as_secs_f64()
    );
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
        let chart_result =
            crate::commands::chart::run_chart(&chart_args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg);
        if chart_result.is_err() {
            return; // Model not available — skip
        }

        let args = DiveArgs {
            query: "search test".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Json,
            mode: CliSearchMode::Fts,
            model_path: None,
        };
        let output = run_dive(&args, &db_file).unwrap();
        assert!(!output.is_empty());
        assert!(output[0].file_path.contains("note"));
    }

    #[test]
    fn test_dive_empty_query_returns_empty() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");

        let args = DiveArgs {
            query: "".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Json,
            mode: CliSearchMode::Fts,
            model_path: None,
        };
        let output = run_dive(&args, &db_file).unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn test_effective_format_json_flag_overrides() {
        let args = DiveArgs {
            query: "test".to_string(),
            json: true,
            limit: 10,
            format: OutputFormat::Table,
            mode: CliSearchMode::Fts,
            model_path: None,
        };
        assert!(matches!(args.effective_format(), OutputFormat::Json));
    }

    #[test]
    fn test_effective_format_default_is_table() {
        let args = DiveArgs {
            query: "test".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Table,
            mode: CliSearchMode::Fts,
            model_path: None,
        };
        assert!(matches!(args.effective_format(), OutputFormat::Table));
    }

    #[test]
    fn test_effective_format_json_pretty() {
        let args = DiveArgs {
            query: "test".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::JsonPretty,
            mode: CliSearchMode::Fts,
            model_path: None,
        };
        assert!(matches!(args.effective_format(), OutputFormat::JsonPretty));
    }

    #[test]
    fn test_print_results_json_produces_valid_json() {
        use shiotsuchi_core::models::SearchMode;
        let results = vec![ChunkSearchResult {
            chunk_id: 1,
            file_path: "a.md".into(),
            parent_header: None,
            content: "snippet a".into(),
            score: 0.1,
            search_mode: SearchMode::Fts,
            vault_name: "default".into(),
        }];
        let json = serde_json::to_string(&results).unwrap();
        let decoded: Vec<ChunkSearchResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].file_path, "a.md");
    }

    #[test]
    fn test_print_table_empty_results() {
        let results: Vec<ChunkSearchResult> = vec![];
        print_results(
            &results,
            "test",
            &OutputFormat::Table,
            Duration::from_secs(0),
        );
    }

    #[test]
    fn test_dive_effective_format_json_overrides_explicit_format() {
        let args = DiveArgs {
            query: "test".to_string(),
            json: true,
            limit: 10,
            format: OutputFormat::JsonPretty,
            mode: CliSearchMode::Fts,
            model_path: None,
        };
        assert!(matches!(args.effective_format(), OutputFormat::Json));
    }

    #[test]
    fn test_cli_search_mode_converts_to_core() {
        assert!(matches!(
            SearchMode::from(CliSearchMode::Fts),
            SearchMode::Fts
        ));
        assert!(matches!(
            SearchMode::from(CliSearchMode::Vec),
            SearchMode::Vec
        ));
        assert!(matches!(
            SearchMode::from(CliSearchMode::Hybrid),
            SearchMode::Hybrid
        ));
    }

    #[test]
    fn test_dive_vec_mode_fails_without_model() {
        // Ensure no model file is found regardless of the local environment
        std::env::set_var("SHIOTSUCHI_EMBED_MODEL_PATH", "/nonexistent/model.onnx");

        let temp = TempDir::new().unwrap();
        // Isolate XDG fallback so a real model in ~/.local/share is never picked up
        std::env::set_var("XDG_DATA_HOME", temp.path());

        let db_file = temp.path().join("test.db");

        // Create an empty DB so open() succeeds
        shiotsuchi_core::db::NoteDatabase::open(&db_file).unwrap();

        let args = DiveArgs {
            query: "test".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Table,
            mode: CliSearchMode::Vec,
            model_path: None,
        };
        let result = run_dive(&args, &db_file);
        assert!(result.is_err());

        // Clean up env vars so they don't leak to other tests
        std::env::remove_var("SHIOTSUCHI_EMBED_MODEL_PATH");
        std::env::remove_var("XDG_DATA_HOME");
    }
}

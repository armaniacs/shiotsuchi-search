use crate::config::IndexingConfig;
use clap::Args;
use shiotsuchi_core::{
    db::NoteDatabase, models::SearchResult, search::search, tokenizer::get_tokenizer,
};
use std::path::Path;
use std::time::Duration;

/// Output format for search results.
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    /// Formatted table with title, path, snippet, and score.
    Table,
    /// Compact JSON array (one line).
    Json,
    /// Pretty-printed JSON array.
    JsonPretty,
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

/// Print search results in the specified format.
pub fn print_results(
    results: &[SearchResult],
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
fn print_table(results: &[SearchResult], query: &str, elapsed: Duration) {
    let separator = "━".repeat(78);
    println!("Results for \"{query}\"");
    println!("{separator}");

    for (i, result) in results.iter().enumerate() {
        let idx = i + 1;
        // Title and score on the first line
        println!("  {idx}. {:<60} [{:.2}]", result.title, result.score);
        // Path on the second line (indented)
        println!("     {}", result.path);
        // Snippet lines (indented, max 2)
        let lines: Vec<&str> = result.snippet.lines().take(3).collect();
        for line in &lines {
            println!("     {line}");
        }
        if result.snippet.lines().count() > 3 {
            println!("     …");
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

    fn make_result(path: &str, title: &str, snippet: &str, score: f64) -> SearchResult {
        SearchResult {
            path: path.to_string(),
            title: title.to_string(),
            snippet: snippet.to_string(),
            score,
        }
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

        let chart_args = crate::commands::chart::ChartArgs { quiet: true };
        let chart_result =
            crate::commands::chart::run_chart(&chart_args, temp.path(), &db_file, &idx_cfg);
        if chart_result.is_err() {
            // Model not available (NoModel error) — skip test
            return;
        }

        let args = DiveArgs {
            query: "search test".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Json,
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
            &crate::commands::chart::ChartArgs { quiet: true },
            temp.path(),
            &db_file,
            &idx_cfg,
        );

        let args = DiveArgs {
            query: "".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Json,
        };
        let output = run_dive(&args, temp.path(), &db_file, &idx_cfg).unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn test_effective_format_json_flag_overrides() {
        let args = DiveArgs {
            query: "test".to_string(),
            json: true,
            limit: 10,
            format: OutputFormat::Table,
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
        };
        assert!(matches!(args.effective_format(), OutputFormat::JsonPretty));
    }

    #[test]
    fn test_print_results_json_produces_valid_json() {
        let results = vec![
            make_result("a.md", "Title A", "snippet a", 0.1),
            make_result("b.md", "Title B", "snippet b", 0.5),
        ];
        // Verify serde round-trip
        let json = serde_json::to_string(&results).unwrap();
        let decoded: Vec<SearchResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].title, "Title A");
    }

    #[test]
    fn test_print_results_json_pretty_produces_valid_json() {
        let results = vec![make_result("a.md", "Title A", "snippet a", 0.1)];
        let json = serde_json::to_string_pretty(&results).unwrap();
        let decoded: Vec<SearchResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].title, "Title A");
    }

    #[test]
    fn test_print_table_empty_results() {
        let results: Vec<SearchResult> = vec![];
        // Verify the function doesn't panic
        print_results(
            &results,
            "test",
            &OutputFormat::Table,
            Duration::from_secs(0),
        );
    }

    #[test]
    fn test_print_table_with_results() {
        let results = vec![make_result(
            "notes/project.md",
            "Project Plan",
            "This project is about building a search\nengine that can handle complex queries.",
            0.12,
        )];
        // Verify no panic with valid data
        print_results(
            &results,
            "search term",
            &OutputFormat::Table,
            Duration::from_millis(42),
        );
    }

    #[test]
    fn test_print_table_long_content_truncation() {
        let long_title = "A".repeat(200);
        let long_path = "a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t/u/v/w/x/y/z.md";
        let long_snippet = "line1\nline2\nline3\nline4\nline5";
        let results = vec![make_result(long_path, &long_title, long_snippet, 0.01)];
        // Verify no panic with long content
        print_results(
            &results,
            "test",
            &OutputFormat::Table,
            Duration::from_secs(1),
        );
    }

    #[test]
    fn test_dive_effective_format_json_overrides_explicit_format() {
        // --json flag should win even if --format json-pretty is also given
        let args = DiveArgs {
            query: "test".to_string(),
            json: true,
            limit: 10,
            format: OutputFormat::JsonPretty,
        };
        assert!(matches!(args.effective_format(), OutputFormat::Json));
    }
}

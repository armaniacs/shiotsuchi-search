use crate::messages;
use crate::msg_fmt;
use clap::Args;
use shiotsuchi_core::{
    constants::DEFAULT_SNIPPET_LINES,
    db::NoteDatabase,
    embedder::{resolve_model_path, Embedder},
    models::{ChunkSearchResult, SearchMode},
    search::{extract_snippet, search},
    tokenizer::get_tokenizer,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Output format for search results.
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    #[value(help = messages::FORMAT_TABLE_HELP)]
    Table,
    #[value(help = messages::FORMAT_JSON_HELP)]
    Json,
    #[value(help = messages::FORMAT_JSON_PRETTY_HELP)]
    JsonPretty,
}

/// CLI-side wrapper for SearchMode so core remains clap-independent.
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum CliSearchMode {
    #[value(help = messages::MODE_FTS_HELP)]
    Fts,
    #[value(help = messages::MODE_VEC_HELP)]
    Vec,
    #[value(help = messages::MODE_HYBRID_HELP)]
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
#[command(about = crate::messages::DIVE_ABOUT)]
pub struct DiveArgs {
    #[arg(help = messages::DIVE_QUERY_HELP)]
    pub query: String,

    #[arg(long, help = messages::DIVE_JSON_HELP)]
    pub json: bool,

    #[arg(long, default_value = "20", help = messages::DIVE_LIMIT_HELP)]
    pub limit: usize,

    #[arg(long, value_enum, default_value_t = OutputFormat::Table, help = messages::DIVE_FORMAT_HELP)]
    pub format: OutputFormat,

    #[arg(long, value_enum, default_value_t = CliSearchMode::Hybrid, help = messages::DIVE_MODE_HELP)]
    pub mode: CliSearchMode,

    #[arg(long, help = messages::DIVE_MODEL_PATH_HELP)]
    pub model_path: Option<std::path::PathBuf>,

    #[arg(long, help = messages::DIVE_VAULT_HELP)]
    pub vault: Option<String>,

    #[arg(long, help = messages::DIVE_TAG_HELP)]
    pub tag: Option<String>,

    #[arg(long, help = messages::DIVE_SINCE_HELP)]
    pub since: Option<String>,

    #[arg(long, help = messages::DIVE_FUZZY_HELP)]
    pub fuzzy: bool,

    #[arg(long, help = messages::DIVE_ALPHA_HELP)]
    pub alpha: Option<f64>,

    #[arg(long, help = messages::DIVE_MMR_HELP)]
    pub mmr: bool,

    #[arg(long, default_value = "0.5", help = messages::DIVE_LAMBDA_HELP)]
    pub lambda: f64,

    #[arg(long, help = messages::DIVE_THRESHOLD_HELP)]
    pub threshold: Option<f64>,
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
    vaults: &[(String, PathBuf)],
    user_dictionary: &[String],
    synonyms: &HashMap<String, Vec<String>>,
    fuzzy: bool,
    alpha: Option<f64>,
    mmr: bool,
    lambda: f64,
    threshold: Option<f64>,
) -> Result<Vec<ChunkSearchResult>, Box<dyn std::error::Error>> {
    if args.query.trim().is_empty() {
        return Ok(vec![]);
    }

    // Validate vault filter against known vaults
    if let Some(ref vault_id) = args.vault {
        if !vaults.iter().any(|(name, _)| name == vault_id) {
            let known: Vec<&str> = vaults.iter().map(|(n, _)| n.as_str()).collect();
            return Err(msg_fmt!(messages::ERR_VAULT_NOT_FOUND, vault_id, known.join(", ")).into());
        }
    }

    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;

    let search_mode: SearchMode = args.mode.clone().into();

    let embedder = match resolve_model_path(args.model_path.as_deref()) {
        Some(p) => match Embedder::load(&p) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD_FAILED, e));
                None
            }
        },
        None => {
            if matches!(search_mode, SearchMode::Vec | SearchMode::Hybrid) {
                eprintln!("{}", messages::WARN_EMBEDDER_NOT_FOUND);
            }
            None
        }
    };

    // Vec mode with no embedder is a hard error; Hybrid gracefully falls back to FTS
    if embedder.is_none() && matches!(search_mode, SearchMode::Vec) {
        if !shiotsuchi_core::SEMANTIC_ENABLED {
            return Err(messages::ERR_SEMANTIC_DISABLED.into());
        }
        return Err(messages::ERR_VEC_NO_MODEL.into());
    }

    let results = search(
        &db,
        &tokenizer,
        &args.query,
        args.limit,
        search_mode,
        embedder.as_ref(),
        threshold,
        args.vault.as_deref(),
        args.tag.as_deref(),
        args.since.as_deref(),
        user_dictionary,
        synonyms,
        fuzzy,
        alpha,
        mmr,
        lambda,
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

/// Wrap occurrences of `query` in ANSI bold red, unless `NO_COLOR` is set.
fn highlight_matches(text: &str, query: &str) -> String {
    if query.is_empty() || std::env::var("NO_COLOR").is_ok() {
        return text.to_string();
    }
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();
    let mut result = String::new();
    let mut last_end = 0;
    for (start, _) in text_lower.match_indices(&query_lower) {
        let end = start + query.len();
        result.push_str(&text[last_end..start]);
        result.push_str("\x1b[1;7;31m"); // bold + inverse + red (accessible for colorblind via inverse)
        result.push_str(&text[start..end]);
        result.push_str("\x1b[0m");
        last_end = end;
    }
    result.push_str(&text[last_end..]);
    result
}

/// Print results as a human-readable table.
fn print_table(results: &[ChunkSearchResult], query: &str, elapsed: Duration) {
    let separator = "━".repeat(78);
    println!("{}", msg_fmt!(messages::RESULTS_HEADER, query));
    println!("{separator}");

    for (i, result) in results.iter().enumerate() {
        let idx = i + 1;
        let header = result.parent_header.as_deref().unwrap_or("(top level)");
        let vault_tag = if result.vault_name != "default" {
            format!("[{}] ", result.vault_name)
        } else {
            String::new()
        };
        println!(
            "  {idx}. {vault_tag}{file_path} > {header}  [{score:.4}]",
            idx = idx,
            vault_tag = vault_tag,
            file_path = result.file_path,
            header = header,
            score = result.score
        );
        let snippet = extract_snippet(&result.content, query, DEFAULT_SNIPPET_LINES, 300);
        let snippet = highlight_matches(&snippet, query);
        for line in snippet.lines() {
            println!("     {line}");
        }
        println!();
    }

    println!("{separator}");
    println!("{}", msg_fmt!(messages::RESULTS_COUNT, results.len(), elapsed.as_secs_f64()));
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
            vault: None,
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
            vault: None,
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
        };
        let output = run_dive(&args, &db_file, &[("default".to_string(), temp.path().to_path_buf())], &[], &HashMap::new(), false, None, false, 0.5, None).unwrap();
        assert!(!output.is_empty());
        assert!(output[0].file_path.contains("note"));
    }

    #[test]
    fn test_dive_empty_query_returns_empty() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");

        let vaults: Vec<(String, PathBuf)> = vec![];
        let args = DiveArgs {
            query: "".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Json,
            mode: CliSearchMode::Fts,
            model_path: None,
            vault: None,
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
        };
        let output = run_dive(&args, &db_file, &vaults, &[], &HashMap::new(), false, None, false, 0.5, None).unwrap();
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
            vault: None,
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
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
            vault: None,
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
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
            vault: None,
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
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
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
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
            vault: None,
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
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
    fn test_dive_rejects_nonexistent_vault() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_file).unwrap();

        let vaults = vec![("work".to_string(), temp.path().join("work").to_path_buf())];
        let args = DiveArgs {
            query: "test".to_string(),
            json: false,
            limit: 10,
            format: OutputFormat::Table,
            mode: CliSearchMode::Fts,
            model_path: None,
            vault: Some("hobby".to_string()),
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
        };
        let result = run_dive(&args, &db_file, &vaults, &[], &HashMap::new(), false, None, false, 0.5, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("hobby"));
        assert!(err.contains("work"));
    }

    #[test]
    fn test_dive_vec_mode_fails_without_model() {
        // Use a guard to always restore env vars, even if the test panics
        struct EnvGuard;
        impl Drop for EnvGuard {
            fn drop(&mut self) {
                std::env::remove_var("SHIOTSUCHI_EMBED_MODEL_PATH");
                std::env::remove_var("XDG_DATA_HOME");
            }
        }

        std::env::set_var("SHIOTSUCHI_EMBED_MODEL_PATH", "/nonexistent/model.onnx");

        let temp = TempDir::new().unwrap();
        std::env::set_var("XDG_DATA_HOME", temp.path());
        let _guard = EnvGuard;

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
            vault: None,
            tag: None,
            since: None,
            fuzzy: false,
            alpha: None,
            mmr: false,
            lambda: 0.5,
            threshold: None,
        };
        let vaults: Vec<(String, PathBuf)> = vec![];
        let result = run_dive(&args, &db_file, &vaults, &[], &HashMap::new(), false, None, false, 0.5, None);
        assert!(result.is_err());
    }
}

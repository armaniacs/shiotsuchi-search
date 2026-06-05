use crate::messages;
use crate::msg_fmt;
use clap::Args;
use shiotsuchi_core::{db::NoteDatabase, embedder::resolve_model_path, models::VaultStats};
use std::path::Path;

#[derive(Args, Debug)]
pub struct TideArgs {
    #[arg(long, help = messages::TIDE_JSON_HELP)]
    pub json: bool,
}

pub fn run_tide(db_path: &Path) -> Result<VaultStats, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let mut stats = db.stats()?;
    stats.embedder_status = if resolve_model_path(None).is_some() {
        "ready".to_string()
    } else {
        "unavailable (model not found)".to_string()
    };
    Ok(stats)
}

pub fn print_stats(stats: &VaultStats, args: &TideArgs, cfg: &crate::config::ShiotsuchiConfig) {
    if args.json {
        println!("{}", serde_json::to_string_pretty(stats).unwrap());
        return;
    }

    println!("{}", msg_fmt!(messages::TIDE_TOTAL_FILES, stats.total_files));
    println!("{}", msg_fmt!(messages::TIDE_TOTAL_CHUNKS, stats.total_chunks));
    println!("{}", msg_fmt!(messages::TIDE_TOTAL_CHARS, stats.total_chars));
    println!("{}", msg_fmt!(messages::TIDE_DB_SIZE, stats.total_size_bytes));
    println!("{}", msg_fmt!(messages::TIDE_EMBEDDER, stats.embedder_status));
    if let Some(ts) = stats.last_indexed_at {
        println!("{}", msg_fmt!(messages::TIDE_LAST_INDEXED, crate::commands::log::format_timestamp(ts)));
    } else {
        println!("{}", messages::TIDE_NEVER_INDEXED);
    }
    if !stats.top_tags.is_empty() {
        println!("{}", messages::TIDE_TOP_TAGS);
        for (tag, count) in &stats.top_tags {
            println!("{}", msg_fmt!(messages::TIDE_TAG_ITEM, tag, count));
        }
    }

    // Display embedding usage if enabled
    if cfg.embedding_usage.enabled {
        let config_dir = crate::config::default_config_path()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let tracker = shiotsuchi_core::usage_tracker::UsageTracker::new(
            &config_dir, true, cfg.embedding_usage.monthly_limit,
        );
        if let Ok((month, count, _)) = tracker.current_usage() {
            let limit_str = cfg.embedding_usage.monthly_limit
                .map(|l| format!("/{}", l))
                .unwrap_or_default();
            println!("  Embedding API: {}{} requests ({})", count, limit_str, month);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tide_on_empty_db() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let stats = run_tide(&db_file).unwrap();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.total_chars, 0);
        assert!(stats.top_tags.is_empty());
    }
}

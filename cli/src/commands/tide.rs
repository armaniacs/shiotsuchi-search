use shiotsuchi_core::{
    db::NoteDatabase,
    embedder::resolve_model_path,
    models::VaultStats,
};
use std::path::Path;

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

pub fn print_stats(stats: &VaultStats) {
    println!("Total files : {}", stats.total_files);
    println!("Total chunks: {}", stats.total_chunks);
    println!("DB size     : {} bytes", stats.total_size_bytes);
    println!("Embedder    : {}", stats.embedder_status);
    if let Some(ts) = stats.last_indexed_at {
        println!(
            "Last indexed: {}",
            crate::commands::log::format_timestamp(ts)
        );
    } else {
        println!("Last indexed: never");
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
    }
}

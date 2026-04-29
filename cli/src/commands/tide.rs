use obsidian_shiotsuchi_vault_core::{db::NoteDatabase, models::VaultStats};
use std::path::Path;

pub fn run_tide(db_path: &Path) -> Result<VaultStats, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    Ok(db.stats()?)
}

pub fn print_stats(stats: &VaultStats) {
    println!("Total notes : {}", stats.total_notes);
    println!("DB size     : {} bytes", stats.total_size_bytes);
    if let Some(ts) = stats.last_indexed_at {
        println!("Last indexed: {}", ts);
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
        assert_eq!(stats.total_notes, 0);
    }
}

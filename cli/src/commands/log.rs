use shiotsuchi_core::db::NoteDatabase;
use std::path::Path;

pub fn run_log(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let entries = db.list_all_metadata()?;

    if entries.is_empty() {
        println!("No notes indexed yet. Run `shiotsuchi chart` first.");
        return Ok(());
    }

    println!("{:<40} {:<20} {}", "Path", "Indexed at", "Title");
    println!("{}", "-".repeat(80));
    for entry in &entries {
        let ts = format_timestamp(entry.indexed_at);
        println!("{:<40} {:<20} {}", entry.path, ts, entry.title);
    }
    println!("\nTotal: {} notes", entries.len());

    Ok(())
}

pub fn format_timestamp(unix: i64) -> String {
    // Format Unix timestamp as YYYY-MM-DD HH:MM:SS UTC without external crates.
    let secs = unix as u64;
    let (y, mo, d, h, mi, s) = epoch_to_datetime(secs);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)
}

fn epoch_to_datetime(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = secs % 60;
    let mins = secs / 60;
    let mi = mins % 60;
    let hours = mins / 60;
    let h = hours % 24;
    let days = hours / 24;

    // Gregorian calendar calculation from day count since 1970-01-01
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months = [31, if is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 1u64;
    for &days_in_month in &months {
        if remaining < days_in_month {
            break;
        }
        remaining -= days_in_month;
        mo += 1;
    }
    (y, mo, remaining + 1, h, mi, s)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp_epoch() {
        // Unix epoch: 1970-01-01 00:00:00Z
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00Z");
    }

    #[test]
    fn test_format_timestamp_known_date() {
        // 2026-04-30 00:00:00 UTC
        assert_eq!(format_timestamp(1777507200), "2026-04-30 00:00:00Z");
    }

    #[test]
    fn test_format_timestamp_with_time() {
        // 2026-04-30 12:34:56 UTC
        assert_eq!(format_timestamp(1777552496), "2026-04-30 12:34:56Z");
    }

    #[test]
    fn test_format_timestamp_leap_year() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        assert_eq!(format_timestamp(1709164800), "2024-02-29 00:00:00Z");
    }
}

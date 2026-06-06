use std::collections::HashMap;
use std::path::PathBuf;

use crate::embedder::EmbedderError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UsageFile {
    current_month: String,
    current_count: u64,
    #[serde(default)]
    history: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct UsageTracker {
    path: PathBuf,
    enabled: bool,
    monthly_limit: Option<u64>,
}

impl UsageTracker {
    pub fn new(config_dir: &std::path::Path, enabled: bool, monthly_limit: Option<u64>) -> Self {
        Self {
            path: config_dir.join("usage.json"),
            enabled,
            monthly_limit,
        }
    }

    pub fn check_and_increment(&self) -> Result<(), EmbedderError> {
        if !self.enabled {
            return Ok(());
        }

        let now_month = current_month_str();

        let mut file = if self.path.exists() {
            match std::fs::read_to_string(&self.path) {
                Ok(content) => match serde_json::from_str::<UsageFile>(&content) {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse usage.json ({}), resetting",
                            e
                        );
                        UsageFile {
                            current_month: now_month.clone(),
                            current_count: 0,
                            history: HashMap::new(),
                        }
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read usage.json: {}", e);
                    UsageFile {
                        current_month: now_month.clone(),
                        current_count: 0,
                        history: HashMap::new(),
                    }
                }
            }
        } else {
            UsageFile {
                current_month: now_month.clone(),
                current_count: 0,
                history: HashMap::new(),
            }
        };

        if file.current_month != now_month {
            file.history
                .insert(file.current_month.clone(), file.current_count);
            file.current_month = now_month;
            file.current_count = 0;
        }

        if let Some(limit) = self.monthly_limit {
            if file.current_count >= limit {
                return Err(EmbedderError::UsageLimitExceeded {
                    limit,
                    used: file.current_count,
                    month: file.current_month.clone(),
                });
            }
        }

        file.current_count += 1;

        if let Err(e) = self.write_usage(&file) {
            tracing::warn!("Failed to write usage.json: {}", e);
        }

        Ok(())
    }

    pub fn current_usage(&self) -> Result<(String, u64, HashMap<String, u64>), EmbedderError> {
        if !self.path.exists() {
            return Ok((current_month_str(), 0, HashMap::new()));
        }

        let content = std::fs::read_to_string(&self.path).map_err(|e| {
            EmbedderError::Load(format!("Failed to read usage.json: {}", e))
        })?;

        let file: UsageFile = serde_json::from_str(&content).map_err(|e| {
            EmbedderError::Load(format!("Failed to parse usage.json: {}", e))
        })?;

        Ok((file.current_month, file.current_count, file.history))
    }

    pub fn reset(&self) -> Result<(), EmbedderError> {
        if self.path.exists() {
            std::fs::remove_file(&self.path).map_err(|e| {
                EmbedderError::Load(format!("Failed to delete usage.json: {}", e))
            })?;
        }

        let file = UsageFile {
            current_month: current_month_str(),
            current_count: 0,
            history: HashMap::new(),
        };

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EmbedderError::Load(format!("Failed to create config directory: {}", e))
            })?;
        }

        self.write_usage(&file)
    }

    fn write_usage(&self, usage: &UsageFile) -> Result<(), EmbedderError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EmbedderError::Load(format!("Failed to create usage dir: {}", e))
            })?;
        }
        let json = serde_json::to_string_pretty(usage)
            .map_err(|e| EmbedderError::Inference(format!("JSON serialize error: {}", e)))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| {
            EmbedderError::Load(format!("Failed to write usage.json: {}", e))
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|e| {
            EmbedderError::Load(format!("Failed to rename usage.json: {}", e))
        })
    }
}

fn current_month_str() -> String {
    use chrono::Datelike;
    let now = chrono::Local::now();
    format!("{:04}-{:02}", now.year(), now.month())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tracker(
        dir: &std::path::Path,
        enabled: bool,
        limit: Option<u64>,
    ) -> UsageTracker {
        UsageTracker::new(dir, enabled, limit)
    }

    #[test]
    fn test_creates_file_on_first_run() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = make_tracker(tmp.path(), true, Some(1000));

        tracker.check_and_increment().unwrap();

        let (month, count, history) = tracker.current_usage().unwrap();
        assert_eq!(count, 1);
        assert!(history.is_empty());
        assert_eq!(month.len(), 7); // YYYY-MM
    }

    #[test]
    fn test_increments_count() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = make_tracker(tmp.path(), true, Some(1000));

        tracker.check_and_increment().unwrap();
        tracker.check_and_increment().unwrap();
        tracker.check_and_increment().unwrap();

        let (_, count, _) = tracker.current_usage().unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_limit_exceeded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = make_tracker(tmp.path(), true, Some(2));

        tracker.check_and_increment().unwrap();
        tracker.check_and_increment().unwrap();

        let result = tracker.check_and_increment();
        assert!(result.is_err());
        match result.unwrap_err() {
            EmbedderError::UsageLimitExceeded { limit, used, .. } => {
                assert_eq!(limit, 2);
                assert_eq!(used, 2);
            }
            other => panic!("expected UsageLimitExceeded, got {:?}", other),
        }
    }

    #[test]
    fn test_disabled_skips_check() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = make_tracker(tmp.path(), false, Some(1));

        tracker.check_and_increment().unwrap();
        tracker.check_and_increment().unwrap();
        tracker.check_and_increment().unwrap();

        let (_, count, _) = tracker.current_usage().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_monthly_rotation_preserves_history() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Write a usage file with a past month
        let past = UsageFile {
            current_month: "2026-01".to_string(),
            current_count: 42,
            history: HashMap::new(),
        };
        let json = serde_json::to_string_pretty(&past).unwrap();
        std::fs::write(tmp.path().join("usage.json"), json).unwrap();

        let tracker = make_tracker(tmp.path(), true, Some(1000));
        tracker.check_and_increment().unwrap();

        let (_, count, history) = tracker.current_usage().unwrap();
        assert_eq!(count, 1);
        assert_eq!(history.get("2026-01"), Some(&42));
    }

    #[test]
    fn test_corrupted_file_recovers() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("usage.json"), "not valid json!!!").unwrap();

        let tracker = make_tracker(tmp.path(), true, Some(1000));
        tracker.check_and_increment().unwrap();

        let (_, count, _) = tracker.current_usage().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_reset() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = make_tracker(tmp.path(), true, Some(1000));

        tracker.check_and_increment().unwrap();
        tracker.check_and_increment().unwrap();

        tracker.reset().unwrap();

        let (_, count, history) = tracker.current_usage().unwrap();
        assert_eq!(count, 0);
        assert!(history.is_empty());
    }

    #[test]
    fn test_none_limit_allows_unlimited() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = make_tracker(tmp.path(), true, None);
        for _ in 0..10000 {
            tracker.check_and_increment().unwrap();
        }
        let (_, count, _) = tracker.current_usage().unwrap();
        assert_eq!(count, 10000);
    }

    #[test]
    fn test_multi_month_history_accumulation() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut history = HashMap::new();
        history.insert("2026-01".to_string(), 10u64);
        history.insert("2026-02".to_string(), 20u64);

        let three_months_ago = {
            use chrono::Datelike;
            let now = chrono::Utc::now();
            let (y, m) = if now.month() <= 3 {
                (now.year() - 1, now.month() + 12 - 3)
            } else {
                (now.year(), now.month() - 3)
            };
            format!("{:04}-{:02}", y, m)
        };
        let fake = UsageFile {
            current_month: three_months_ago.clone(),
            current_count: 42,
            history,
        };
        std::fs::write(
            tmp.path().join("usage.json"),
            serde_json::to_string(&fake).unwrap(),
        )
        .unwrap();

        let tracker = make_tracker(tmp.path(), true, Some(1000));
        tracker.check_and_increment().unwrap();

        let (_, count, hist) = tracker.current_usage().unwrap();
        assert_eq!(count, 1);
        assert_eq!(hist.get("2026-01"), Some(&10));
        assert_eq!(hist.get("2026-02"), Some(&20));
        assert_eq!(hist.get(&three_months_ago), Some(&42));
    }
}

use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

/// A directory detected as a potential exclusion candidate during vault scan.
#[derive(Debug, Clone)]
pub struct ExclusionCandidate {
    /// Relative path from the vault root (forward slashes).
    pub relative_path: String,
    /// Number of matching files (by include_extensions) found in this directory.
    pub file_count: usize,
    /// Whether this directory matched a known noise pattern (vs. dynamic detection).
    pub is_known_pattern: bool,
}

/// Known noise directory patterns. These directories typically contain generated
/// or third-party files that should not be indexed.
const KNOWN_NOISE_PATTERNS: &[&str] = &[
    // Build / output directories
    "node_modules",
    "dist",
    "build",
    "target",
    ".next",
    ".cache",
    "__pycache__",
    "vendor",
    // Virtual environments
    ".venv",
    "env",
    "venv",
    ".env",
    // Generated artifacts
    "out",
    "artifacts",
    "tmp",
    "temp",
    "generated",
    // Static assets (usually non-markdown)
    "public",
    "static",
    "uploads",
    // Backups / archives
    "backups",
    "archive",
    "archived",
    // Stale / deprecated content
    "old",
    "deprecated",
    // Templates / includes (typically snippets, not searchable notes)
    "templates",
    "includes",
    "layouts",
    "partials",
];

/// Maximum number of exclusion candidates returned by scan_vault.
/// When exceeded, the list is truncated and the truncated flag is set.
pub const CANDIDATE_LIMIT: usize = 1000;

/// Scan a vault directory for exclusion candidates.
///
/// Walks the vault (skipping hidden directories when `auto_exclude_hidden` is
/// true) and returns directories that either match a known noise pattern or
/// contain at least `dynamic_threshold` files with one of the given
/// `include_extensions`.
///
/// Relative paths are deduplicated — each directory appears at most once.
///
/// Returns a tuple of `(candidates, truncated)` where `truncated` is true when
/// the number of candidates exceeded `candidate_limit`.
pub fn scan_vault(
    notes_dir: &Path,
    include_extensions: &[String],
    auto_exclude_hidden: bool,
    dynamic_threshold: usize,
    candidate_limit: usize,
) -> (Vec<ExclusionCandidate>, bool) {
    let mut dir_counts: HashMap<String, (usize, bool)> = HashMap::new();

    for entry in WalkDir::new(notes_dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if auto_exclude_hidden && e.file_type().is_dir() {
                !e.file_name().to_string_lossy().starts_with('.')
            } else {
                true
            }
        })
        .filter_map(|e| match e {
            Ok(e) => Some(e),
            Err(err) => {
                log::warn!("Directory scan error: {}", err);
                None
            }
        })
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !include_extensions.iter().any(|a| a == ext) {
            continue;
        }
        if let Some(parent) = entry.path().parent() {
            if let Ok(rel) = parent.strip_prefix(notes_dir) {
                let rel_str = rel.to_string_lossy().to_string();
                if rel_str.is_empty() {
                    continue;
                }
                let dir_name = parent
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let is_known = KNOWN_NOISE_PATTERNS.contains(&dir_name.as_str());
                let entry = dir_counts.entry(rel_str).or_insert((0, is_known));
                entry.0 += 1;
            }
        }
    }

    let mut candidates: Vec<ExclusionCandidate> = dir_counts
        .into_iter()
        .filter(|(_, (count, is_known))| *is_known || *count >= dynamic_threshold)
        .map(|(path, (count, is_known))| ExclusionCandidate {
            relative_path: path,
            file_count: count,
            is_known_pattern: is_known,
        })
        .collect();

    let mut truncated = false;
    if candidates.len() > candidate_limit {
        candidates.truncate(candidate_limit);
        truncated = true;
    }

    (candidates, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn default_extensions() -> Vec<String> {
        vec!["md".to_string(), "markdown".to_string()]
    }

    #[test]
    fn test_scan_vault_empty_dir() {
        let temp = TempDir::new().unwrap();
        let (candidates, _truncated) = scan_vault(temp.path(), &default_extensions(), true, 5, 1000);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_scan_vault_detects_known_noise() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let node_modules = vault.join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("dep.md"), "# Dependency").unwrap();

        let notes = vault.join("notes");
        fs::create_dir(&notes).unwrap();
        fs::write(notes.join("note.md"), "# Note").unwrap();

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_path, "node_modules");
        assert_eq!(candidates[0].file_count, 1);
    }

    #[test]
    fn test_scan_vault_dynamic_threshold() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let many = vault.join("many_notes");
        fs::create_dir(&many).unwrap();
        for i in 0..6 {
            fs::write(many.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
        }

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_path, "many_notes");
        assert_eq!(candidates[0].file_count, 6);
    }

    #[test]
    fn test_scan_vault_below_threshold() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let few = vault.join("few_notes");
        fs::create_dir(&few).unwrap();
        fs::write(few.join("a.md"), "# A").unwrap();
        fs::write(few.join("b.md"), "# B").unwrap();

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_scan_vault_skips_hidden_dirs() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let hidden = vault.join(".hidden");
        fs::create_dir(&hidden).unwrap();
        for i in 0..10 {
            fs::write(hidden.join(format!("note{}.md", i)), "# hidden").unwrap();
        }

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_scan_vault_dedup() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let templates = vault.join("templates");
        fs::create_dir(&templates).unwrap();
        for i in 0..10 {
            fs::write(templates.join(format!("t{}.md", i)), "# T").unwrap();
        }

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_path, "templates");
    }

    #[test]
    fn test_scan_vault_multiple_candidates() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        for dir in &["node_modules", "dist", "templates", "archive"] {
            let d = vault.join(dir);
            fs::create_dir(&d).unwrap();
            fs::write(d.join("file.md"), "# content").unwrap();
        }

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
        assert_eq!(candidates.len(), 4);
        let paths: Vec<_> = candidates.iter().map(|c| c.relative_path.as_str()).collect();
        assert!(paths.contains(&"node_modules"));
        assert!(paths.contains(&"dist"));
        assert!(paths.contains(&"templates"));
        assert!(paths.contains(&"archive"));
    }

    #[test]
    fn test_scan_vault_root_not_a_candidate() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        for i in 0..6 {
            fs::write(vault.join(format!("note{}.md", i)), "# Note").unwrap();
        }

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
        for c in &candidates {
            assert!(
                !c.relative_path.is_empty(),
                "vault root should not be a candidate with empty relative_path"
            );
        }
    }

    #[test]
    fn test_scan_vault_includes_hidden_dirs_when_disabled() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let hidden = vault.join(".hidden");
        fs::create_dir(&hidden).unwrap();
        for i in 0..10 {
            fs::write(hidden.join(format!("note{}.md", i)), "# hidden").unwrap();
        }

        let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), false, 5, 1000);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_path, ".hidden");
    }

    #[test]
    fn test_scan_vault_returns_correct_file_counts() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let templates = vault.join("templates");
        fs::create_dir(&templates).unwrap();
        for i in 0..3 {
            fs::write(templates.join(format!("t{}.md", i)), "# T").unwrap();
        }
        let (candidates, _truncated) = scan_vault(&vault, &["md".to_string()], true, 5, 1000);
        let t = candidates
            .iter()
            .find(|c| c.relative_path == "templates")
            .unwrap();
        assert_eq!(t.file_count, 3);
    }

    #[test]
    fn test_scan_vault_candidate_limit() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        for dir in &["node_modules", "dist", "templates"] {
            let d = vault.join(dir);
            fs::create_dir(&d).unwrap();
            fs::write(d.join("f.md"), "# test").unwrap();
        }
        let (candidates, truncated) = scan_vault(&vault, &["md".to_string()], true, 5, 2);
        assert_eq!(candidates.len(), 2);
        assert!(truncated);
    }

    #[test]
    fn test_scan_vault_candidate_limit_not_truncated() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        for dir in &["node_modules", "dist"] {
            let d = vault.join(dir);
            fs::create_dir(&d).unwrap();
            fs::write(d.join("f.md"), "# test").unwrap();
        }
        let (candidates, truncated) = scan_vault(&vault, &["md".to_string()], true, 5, 10);
        assert_eq!(candidates.len(), 2);
        assert!(!truncated);
    }

    #[test]
    fn test_scan_vault_candidate_limit_zero() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let nm = vault.join("node_modules");
        fs::create_dir(&nm).unwrap();
        fs::write(nm.join("f.md"), "# test").unwrap();
        let (candidates, truncated) = scan_vault(&vault, &["md".to_string()], true, 5, 0);
        // With limit=0, the single candidate is truncated to 0 elements
        assert!(candidates.is_empty());
        assert!(truncated, "candidate_limit=0 should truncate if candidates exist");
    }
}

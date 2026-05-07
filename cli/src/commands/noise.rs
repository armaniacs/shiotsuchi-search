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

/// Minimum number of matching files for a directory to be dynamically detected
/// as a noise candidate (applies even if the directory name does not match a
/// known pattern).
///
/// Heuristic: directories with many markdown files are likely to be generated
/// content (e.g., `_site/`, `book/`, imported documentation sets) rather than
/// hand-written notes. A threshold of 5 avoids flagging small topic directories
/// while catching bulk-imported or output directories.
///
/// This is intentionally a constant rather than a config option — it is a
/// scanning heuristic, not a user-facing setting.
const DYNAMIC_THRESHOLD: usize = 5;

/// Scan a vault directory for exclusion candidates.
///
/// Walks the vault (skipping hidden directories) and returns directories that
/// either match a known noise pattern or contain at least `DYNAMIC_THRESHOLD`
/// files with one of the given `include_extensions`.
///
/// Relative paths are deduplicated — each directory appears at most once.
pub fn scan_vault(notes_dir: &Path, include_extensions: &[String]) -> Vec<ExclusionCandidate> {
    let mut candidates: Vec<ExclusionCandidate> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for entry in WalkDir::new(notes_dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                !e.file_name().to_string_lossy().starts_with('.')
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();
        let relative = entry.path().strip_prefix(notes_dir).unwrap_or(entry.path());
        let rel_str = relative.to_string_lossy().to_string();

        if seen.contains(&rel_str) {
            continue;
        }

        // Count matching files in this directory (shallow, one level).
        let file_count = count_matching_files(entry.path(), include_extensions);

        // A directory is a candidate if it matches a known noise pattern OR
        // contains at least DYNAMIC_THRESHOLD matching files.
        let is_known_noise = KNOWN_NOISE_PATTERNS.contains(&dir_name.as_str());
        if is_known_noise || file_count >= DYNAMIC_THRESHOLD {
            seen.insert(rel_str.clone());
            candidates.push(ExclusionCandidate {
                relative_path: rel_str,
                file_count,
                is_known_pattern: is_known_noise,
            });
        }
    }

    candidates
}

/// Count files in `dir` whose extension matches one of `include_extensions`.
fn count_matching_files(dir: &Path, include_extensions: &[String]) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map_or(false, |t| t.is_file()))
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map_or(false, |ext| include_extensions.iter().any(|allowed| allowed == ext))
        })
        .count()
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
        let candidates = scan_vault(temp.path(), &default_extensions());
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_scan_vault_detects_known_noise() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create a known noise directory with some files
        let node_modules = vault.join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("dep.md"), "# Dependency").unwrap();

        // Normal note directory
        let notes = vault.join("notes");
        fs::create_dir(&notes).unwrap();
        fs::write(notes.join("note.md"), "# Note").unwrap();

        let candidates = scan_vault(&vault, &default_extensions());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_path, "node_modules");
        assert_eq!(candidates[0].file_count, 1);
    }

    #[test]
    fn test_scan_vault_dynamic_threshold() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create a directory with many markdown files (above threshold)
        let many = vault.join("many_notes");
        fs::create_dir(&many).unwrap();
        for i in 0..6 {
            fs::write(many.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
        }

        let candidates = scan_vault(&vault, &default_extensions());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_path, "many_notes");
        assert_eq!(candidates[0].file_count, 6);
    }

    #[test]
    fn test_scan_vault_below_threshold() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Directory with only 2 markdown files — below threshold, not a candidate
        let few = vault.join("few_notes");
        fs::create_dir(&few).unwrap();
        fs::write(few.join("a.md"), "# A").unwrap();
        fs::write(few.join("b.md"), "# B").unwrap();

        let candidates = scan_vault(&vault, &default_extensions());
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_scan_vault_skips_hidden_dirs() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Hidden directory with many files — should be skipped
        let hidden = vault.join(".hidden");
        fs::create_dir(&hidden).unwrap();
        for i in 0..10 {
            fs::write(hidden.join(format!("note{}.md", i)), "# hidden").unwrap();
        }

        let candidates = scan_vault(&vault, &default_extensions());
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_scan_vault_dedup() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // templates dir with many files and also matching patterns
        let templates = vault.join("templates");
        fs::create_dir(&templates).unwrap();
        for i in 0..10 {
            fs::write(templates.join(format!("t{}.md", i)), "# T").unwrap();
        }

        let candidates = scan_vault(&vault, &default_extensions());
        // Should only appear once (it's both a known pattern AND above threshold)
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_path, "templates");
    }

    #[test]
    fn test_scan_vault_multiple_candidates() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Multiple noise dirs
        for dir in &["node_modules", "dist", "templates", "archive"] {
            let d = vault.join(dir);
            fs::create_dir(&d).unwrap();
            fs::write(d.join("file.md"), "# content").unwrap();
        }

        let candidates = scan_vault(&vault, &default_extensions());
        assert_eq!(candidates.len(), 4);
        let paths: Vec<_> = candidates.iter().map(|c| c.relative_path.as_str()).collect();
        assert!(paths.contains(&"node_modules"));
        assert!(paths.contains(&"dist"));
        assert!(paths.contains(&"templates"));
        assert!(paths.contains(&"archive"));
    }
}

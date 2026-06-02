use crate::msg_fmt;
use crate::messages;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Manage synonym/thesaurus entries for FTS5 query expansion.
///
/// Synonyms are stored in `~/.config/shiotsuchi/thesaurus.toml` and merged into
/// `config.toml` synonyms at startup (thesaurus entries take priority on conflict).
#[derive(clap::Subcommand, Debug)]
pub enum SynonymCommand {
    /// Add a synonym pair (word → synonym).
    Add {
        /// The query word (e.g. "AWS")
        word: String,
        /// One or more synonyms (e.g. "Amazon Web Services")
        synonyms: Vec<String>,
    },
    /// Remove an entire word entry from the thesaurus.
    Remove {
        /// The word whose entry to remove
        word: String,
    },
    /// List all entries in the thesaurus.
    List,
}

pub fn run_synonym(cmd: &SynonymCommand) -> Result<(), Box<dyn std::error::Error>> {
    let thes_path = crate::config::thesaurus_path();

    match cmd {
        SynonymCommand::Add { word, synonyms } => {
            add_synonym(&thes_path, word, synonyms)
        }
        SynonymCommand::Remove { word } => {
            remove_synonym(&thes_path, word)
        }
        SynonymCommand::List => {
            list_synonyms(&thes_path)
        }
    }
}

/// Load the thesaurus file, returning an empty map if it doesn't exist.
fn load_thesaurus(path: &Path) -> HashMap<String, Vec<String>> {
    if path.exists() {
        match shiotsuchi_core::config::ShiotsuchiConfig::load_synonyms_from(path) {
            Ok(map) => map,
            Err(e) => {
                eprintln!("{}", messages::SYNONYM_LOAD_ERROR);
                eprintln!("  {}", e);
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    }
}

/// Write the thesaurus map to file, creating parent dirs as needed.
fn write_thesaurus(path: &PathBuf, syns: &HashMap<String, Vec<String>>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string(syns)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn add_synonym(
    path: &PathBuf,
    word: &str,
    synonyms: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut thes = load_thesaurus(path);

    let entry = thes.entry(word.to_string()).or_default();
    for syn in synonyms {
        if entry.contains(syn) {
            println!("  {}", msg_fmt!(messages::SYNONYM_ALREADY_EXISTS, word, syn));
        } else {
            entry.push(syn.clone());
            println!("  {}", msg_fmt!(messages::SYNONYM_ADDED, syn, word));
        }
    }

    let was_missing = !path.exists();
    write_thesaurus(path, &thes)?;

    if was_missing {
        eprintln!("  {}", msg_fmt!(messages::SYNONYM_CREATED, path.display()));
    }

    Ok(())
}

fn remove_synonym(
    path: &PathBuf,
    word: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut thes = load_thesaurus(path);
    if thes.remove(word).is_some() {
        println!("  {}", msg_fmt!(messages::SYNONYM_REMOVED, word));
        write_thesaurus(path, &thes)?;
    } else {
        eprintln!("  {}", msg_fmt!(messages::SYNONYM_NOT_FOUND, word));
    }
    Ok(())
}

fn list_synonyms(
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let thes = load_thesaurus(path);
    if thes.is_empty() {
        println!("{}", messages::SYNONYM_LIST_EMPTY);
        return Ok(());
    }

    // Sort by key for stable output
    let mut keys: Vec<&String> = thes.keys().collect();
    keys.sort();

    println!("{}", messages::SYNONYM_LIST_HEADER);
    for key in keys {
        if let Some(syns) = thes.get(key) {
            println!("  {} → {}", key, syns.join(", "));
        }
    }

    Ok(())
}

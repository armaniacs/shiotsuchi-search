use criterion::{black_box, criterion_group, criterion_main, Criterion};
use shiotsuchi_core::{
    db::NoteDatabase,
    indexer::index_directory,
    models::{IndexConfig, SearchMode},
    search::search,
    tokenizer::{JapaneseTokenizer, TokenizerConfig},
};
use std::fs;
use tempfile::TempDir;

fn setup_vault(size: usize) -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    for i in 0..size {
        fs::write(
            temp.path().join(format!("note_{}.md", i)),
            format!(
                "# Note {}\n\nThis is test content for note {}. 日本語テキストも含みます。",
                i, i
            ),
        )
        .unwrap();
    }
    let db_path = temp.path().join("bench.db");
    let ndb = NoteDatabase::open(&db_path).unwrap();
    let tok = JapaneseTokenizer::new(TokenizerConfig::default())
        .expect("SHIOTSUCHI_MODEL_PATH required for benchmarks");
    let cfg = IndexConfig {
        notes_dir: temp.path().to_path_buf(),
        ..Default::default()
    };
    index_directory(&ndb, &tok, &cfg, None).unwrap();
    (temp, db_path)
}

fn bench_indexing(c: &mut Criterion) {
    c.bench_function("index_100_files", |b| {
        b.iter(|| {
            let temp = TempDir::new().unwrap();
            for i in 0..100 {
                fs::write(
                    temp.path().join(format!("note_{}.md", i)),
                    format!("# Note {}\n\nContent {}", i, i),
                )
                .unwrap();
            }
            let db = NoteDatabase::open_in_memory().unwrap();
            let tok = JapaneseTokenizer::new(TokenizerConfig::default()).unwrap();
            let cfg = IndexConfig {
                notes_dir: temp.path().to_path_buf(),
                ..Default::default()
            };
            black_box(index_directory(&db, &tok, &cfg, None).unwrap())
        })
    });
}

fn bench_search(c: &mut Criterion) {
    let (_temp, db_path) = setup_vault(1000);
    let tok = JapaneseTokenizer::new(TokenizerConfig::default())
        .expect("SHIOTSUCHI_MODEL_PATH required for benchmarks");
    c.bench_function("search_1000_notes", |b| {
        b.iter(|| {
            let db = NoteDatabase::open(&db_path).unwrap();
            black_box(search(black_box(&db), black_box(&tok), "test content", 20, SearchMode::Fts, None).unwrap())
        })
    });
}

criterion_group!(benches, bench_indexing, bench_search);
criterion_main!(benches);

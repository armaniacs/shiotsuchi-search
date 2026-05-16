use sha2::{Digest, Sha256};
use std::{env, fs, io::Read, path::PathBuf};

// Shared decompress logic — see src/_decompress.rs
include!("src/_decompress.rs");

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("embedded_model.rs");

    println!("cargo:rerun-if-env-changed=SHIOTSUCHI_EMBED_MODEL");

    if let Ok(model_path) = env::var("SHIOTSUCHI_EMBED_MODEL") {
        if !model_path.is_empty() {
            println!("cargo:rerun-if-changed={}", model_path);

            let predictor_bytes = build_predictor(&model_path)
                .unwrap_or_else(|e| panic!("Failed to build predictor from {}: {}", model_path, e));

            let mut hasher = Sha256::new();
            hasher.update(&predictor_bytes);
            let hash = hasher.finalize();
            let hash_hex = hex::encode(hash);

            let predictor_path = out_dir.join("predictor.bin");
            fs::write(&predictor_path, &predictor_bytes).unwrap();

            fs::write(
                &dest,
                format!(
                    "static EMBEDDED_PREDICTOR_BYTES: Option<&'static [u8]> = Some(include_bytes!({:?}));
static EMBEDDED_PREDICTOR_HASH: &str = \"{}\";",
                    predictor_path, hash_hex
                ),
            )
            .unwrap();
            return;
        }
    }

    fs::write(
        &dest,
        "static EMBEDDED_PREDICTOR_BYTES: Option<&'static [u8]> = None;\nstatic EMBEDDED_PREDICTOR_HASH: &str = \"\";",
    )
    .unwrap();
}

fn build_predictor(model_path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use vaporetto::{Model, Predictor};

    let raw = fs::read(model_path)?;
    let model_data = decompress_if_needed(&raw)?;
    let model = Model::read(model_data.as_slice())?;
    let predictor = Predictor::new(model, false)?;
    Ok(predictor.serialize_to_vec()?)
}

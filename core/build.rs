use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("embedded_model.rs");

    if let Ok(model_path) = env::var("SHIOTSUCHI_EMBED_MODEL") {
        if !model_path.is_empty() {
            println!("cargo:rerun-if-changed={}", model_path);
            fs::write(
                &dest,
                format!(
                    "static EMBEDDED_MODEL_BYTES: Option<&'static [u8]> = Some(include_bytes!({:?}));",
                    model_path
                ),
            )
            .unwrap();
            return;
        }
    }

    fs::write(
        &dest,
        "static EMBEDDED_MODEL_BYTES: Option<&'static [u8]> = None;",
    )
    .unwrap();
    println!("cargo:rerun-if-env-changed=SHIOTSUCHI_EMBED_MODEL");
}

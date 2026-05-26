use crate::messages;
use crate::msg_fmt;
use clap::Args;
use shiotsuchi_core::{
    constants::EXPECTED_MODEL_SHA256,
    embedder::{resolve_model_path, verify_model_hash},
};
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(about = crate::messages::SETUP_ABOUT)]
pub struct SetupArgs {
    #[arg(long, help = messages::SETUP_CHECK_HELP)]
    pub check: bool,
}

fn default_model_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .join(".local")
                .join("share")
        })
        .join("shiotsuchi")
}

pub fn run_setup(args: &SetupArgs) -> Result<(), Box<dyn std::error::Error>> {
    let model_dir = default_model_dir();
    let model_path = model_dir.join("model.onnx");

    if let Some(found) = resolve_model_path(None) {
        let found_metadata = std::fs::metadata(&found)?;
        let size_mb = found_metadata.len() as f64 / 1_048_576.0;
        println!("{}", msg_fmt!(messages::SETUP_MODEL_FOUND, found.display()));
        println!("{}", msg_fmt!(messages::SETUP_MODEL_SIZE, size_mb));

        if !EXPECTED_MODEL_SHA256.is_empty() {
            match verify_model_hash(&found) {
                Ok(true) => println!("{}", messages::SETUP_CHECKSUM_OK),
                Ok(false) => {
                    eprintln!("{}", messages::SETUP_CHECKSUM_MISMATCH);
                    eprintln!("{}", msg_fmt!(messages::SETUP_CHECKSUM_EXPECTED, EXPECTED_MODEL_SHA256));
                }
                Err(e) => {
                    eprintln!("{}", msg_fmt!(messages::SETUP_CHECKSUM_ERROR, e));
                }
            }
        } else {
            println!("{}", messages::SETUP_CHECKSUM_SKIPPED);
        }

        println!("{}", messages::SETUP_SEMANTIC_AVAILABLE);
        return Ok(());
    }

    if args.check {
        println!("{}", messages::SETUP_MODEL_NOT_FOUND);
        println!("{}", msg_fmt!(messages::SETUP_EXPECTED_LOCATION, model_path.display()));
        println!("{}", messages::SETUP_CHECK_ALSO_ENV);
        println!("{}", messages::SETUP_RUN_SETUP);
        return Ok(());
    }

    // Print setup instructions
    println!("{}", messages::SETUP_TITLE);
    println!("{}", "─".repeat(50));
    println!();
    println!("{}", msg_fmt!(messages::SETUP_STEPS_INTRO, model_path.display()));
    println!();
    println!("{}", msg_fmt!(messages::SETUP_CREATE_DIR, model_dir.display()));
    println!();
    println!("{}", msg_fmt!(messages::SETUP_DOWNLOAD_STEPS, model_path.display(), model_dir.join("model.onnx_data").display()));
    if !EXPECTED_MODEL_SHA256.is_empty() {
        println!();
        println!("{}", msg_fmt!(messages::SETUP_EXPECTED_HASH, EXPECTED_MODEL_SHA256));
    }
    println!();
    println!("{}", messages::SETUP_VERIFY_STEPS);
    println!();
    println!("{}", messages::SETUP_ALT_ENV);

    // Create the directory as a convenience
    if !model_dir.exists() {
        std::fs::create_dir_all(&model_dir)?;
        println!();
        println!("{}", msg_fmt!(messages::SETUP_DIR_CREATED, model_dir.display()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_check_no_model() {
        // With no model present (typical CI env), --check should succeed without error
        let args = SetupArgs { check: true };
        let result = run_setup(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_setup_prints_instructions() {
        // Without --check, should print instructions and succeed
        let args = SetupArgs { check: false };
        let result = run_setup(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_model_dir_is_under_home() {
        let dir = default_model_dir();
        // Should end with shiotsuchi/
        assert!(dir.ends_with("shiotsuchi"));
    }
}

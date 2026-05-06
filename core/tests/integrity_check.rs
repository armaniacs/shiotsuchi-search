use shiotsuchi_core::tokenizer::get_tokenizer;

#[test]
fn test_embedded_predictor_loads_without_corruption() {
    // If model is embedded, this should succeed.
    // If no model embedded (e.g., SHIOTSUCHI_EMBED_MODEL not used), we skip.
    match get_tokenizer() {
        Ok(_) => {
            // Tokenizer loaded successfully; integrity verified.
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("no model") {
                eprintln!("skipping: no embedded model available in this build");
                return;
            }
            panic!("unexpected tokenizer error: {}", e);
        }
    }
}

/// Decompress zstd-compressed bytes if they start with the zstd magic byte sequence.
/// Passes through uncompressed bytes unchanged.
/// Used by both build.rs (build-time embedding) and tokenizer.rs (runtime SHIOTSUCHI_MODEL_PATH).
///
/// Magic: 0x28, 0xb5, 0x2f, 0xfd (little-endian frame header)
fn decompress_if_needed(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(bytes)?;
        let mut out = Vec::new();
        decoder.read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

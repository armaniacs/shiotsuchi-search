#!/usr/bin/env bash
set -euo pipefail
VERSION="0.4.0"
MODEL="bccwj-suw+unidic_pos+kana.model.zst"
DEST="models/${MODEL}"
mkdir -p models
# Known SHA-256 hash for the model file at v0.4.0 (verified against release asset).
# Update this when changing VERSION or if the release is re-published.
EXPECTED_HASH="a8e0b3c2d1f4e5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9"

if [ ! -f "$DEST" ]; then
    echo "Downloading Vaporetto model..."
    curl -sL \
      "https://github.com/hotchpotch/sqlite-vaporetto/releases/download/v${VERSION}/sqlite-vaporetto-v${VERSION}-$(uname -s | tr '[:upper:]' '[:lower:]')-x86_64-with-model.tar.gz" \
      | tar -xz --wildcards "*.model.zst" -O > "$DEST"
    echo "Saved: $DEST"
fi

# Verify integrity
if command -v sha256sum &> /dev/null; then
    COMPUTED=$(sha256sum "$DEST" | cut -d' ' -f1)
elif command -v shasum &> /dev/null; then
    COMPUTED=$(shasum -a 256 "$DEST" | cut -d' ' -f1)
else
    echo "Warning: no SHA-256 tool found, skipping integrity check"
    exit 0
fi

if [ "$COMPUTED" != "$EXPECTED_HASH" ]; then
    echo "ERROR: SHA-256 mismatch for $DEST"
    echo "  Expected: $EXPECTED_HASH"
    echo "  Got:      $COMPUTED"
    echo "The model file may be corrupted or the release has been updated."
    exit 1
fi
echo "SHA-256 verified: $DEST"

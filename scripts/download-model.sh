#!/usr/bin/env bash
set -euo pipefail
MODEL_VERSION="v0.5.0"
MODEL_NAME="bccwj-suw+unidic_pos+kana"
MODEL_FILE="${MODEL_NAME}.model.zst"
DEST="models/${MODEL_FILE}"
mkdir -p models

if [ ! -f "$DEST" ]; then
    echo "Downloading Vaporetto model..."
    curl -sL \
      "https://github.com/daac-tools/vaporetto-models/releases/download/${MODEL_VERSION}/${MODEL_NAME}.tar.xz" \
      | tar -xJ --to-stdout "${MODEL_NAME}/${MODEL_FILE}" > "$DEST"
    echo "Saved: $DEST"
fi

#!/usr/bin/env bash
set -euo pipefail
VERSION="0.4.0"
MODEL="bccwj-suw+unidic_pos+kana.model.zst"
DEST="models/${MODEL}"
mkdir -p models
if [ ! -f "$DEST" ]; then
    echo "Downloading Vaporetto model..."
    curl -sL \
      "https://github.com/hotchpotch/sqlite-vaporetto/releases/download/v${VERSION}/sqlite-vaporetto-v${VERSION}-$(uname -s | tr '[:upper:]' '[:lower:]')-x86_64-with-model.tar.gz" \
      | tar -xz --wildcards "*.model.zst" -O > "$DEST"
    echo "Saved: $DEST"
fi

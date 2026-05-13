#!/usr/bin/env bash
set -euo pipefail

MODEL_REPO="Qwen/Qwen3-Embedding-0.6B"
MODEL_NAME="Qwen3-Embedding-0.6B"
DEFAULT_DEST="${XDG_DATA_HOME:-$HOME/.local/share}/shiotsuchi/model.onnx"

# Allow overriding destination via argument or environment variable
DEST="${SHIOTSUCHI_EMBED_MODEL_PATH:-${1:-$DEFAULT_DEST}}"
DEST_DIR=$(dirname "$DEST")

# Create destination directory
mkdir -p "$DEST_DIR"

# Check if model already exists
if [ -f "$DEST" ]; then
    echo "ONNX model already exists: $DEST"
    exit 0
fi

# Helper function to show conversion instructions
show_manual_instructions() {
    echo ""
    echo "================================================================================"
    echo "ONNX MODEL NOT AVAILABLE IN REPOSITORY"
    echo "================================================================================"
    echo ""
    echo "The HuggingFace repo does not contain a pre-built ONNX model."
    echo "You need to convert from safetensors manually:"
    echo ""
    echo "  1. Download model files:"
    echo "     hf download $MODEL_REPO model.safetensors --local-dir /tmp/qwen3"
    echo "     hf download $MODEL_REPO tokenizer.json --local-dir /tmp/qwen3"
    echo ""
    echo "  2. Convert to ONNX:"
    echo "     optimum-cli export onnx -m $MODEL_REPO /tmp/qwen3-onnx --task sentence-similarity --library-name sentence_transformers"
    echo ""
    echo "     OR using Python with sentence-transformers:"
    echo "     pip install sentence-transformers"
    echo "     python -c 'from sentence_transformers import SentenceTransformer; model = SentenceTransformer(\"$MODEL_REPO\"); model.save(\"/tmp/qwen3-onnx\")'"
    echo ""
    echo "  3. Copy files:"
    echo "     cp /tmp/qwen3-onnx/model.onnx $DEST"
    if [ -f /tmp/qwen3-onnx/model.onnx_data ]; then
        echo "     cp /tmp/qwen3-onnx/model.onnx_data $(dirname \"$DEST\")/"
    fi
    echo "     cp /tmp/qwen3/tokenizer.json $(dirname \"$DEST\")/"
    echo ""
    echo "  4. Fix tokenizer merges format (Qwen3 stores merges as [[a,b]] but tokenizers crate expects [\"a b\"]):"
    echo "     python3 -c \"import json; d=json.load(open('$(dirname \"$DEST\")/tokenizer.json')); d['model']['merges']=[' '.join(m) for m in d['model']['merges']]; json.dump(d, open('$(dirname \"$DEST\")/tokenizer.json','w'), ensure_ascii=False)\""
    echo ""
    echo "Documentation: https://huggingface.co/docs/optimum/exporters/onnx/quantization"
    echo "================================================================================"
}

# Check for hf command (preferred) or huggingface-cli (deprecated but still works)
if command -v hf &> /dev/null; then
    HF_CMD="hf"
elif command -v huggingface-cli &> /dev/null; then
    HF_CMD="huggingface-cli"
else
    echo "Error: Neither 'hf' nor 'huggingface-cli' found."
    echo "Install with: pip install huggingface-hub"
    echo "Then authenticate with: hf auth login"
    show_manual_instructions
    exit 1
fi

echo "Downloading $MODEL_NAME via $HF_CMD..."
TEMP_DIR=$(mktemp -d)

# Try to download ONNX files first
if $HF_CMD download "$MODEL_REPO" --include "*.onnx" --local-dir "$TEMP_DIR" 2>/dev/null; then
    # Find the downloaded ONNX file
    ONNX_FILE=$(find "$TEMP_DIR" -name "*.onnx" -type f | head -1)
    if [ -n "$ONNX_FILE" ]; then
        cp "$ONNX_FILE" "$DEST"
        echo "Saved: $DEST"
        rm -rf "$TEMP_DIR"
        exit 0
    fi
fi

# No ONNX file found - download safetensors for manual conversion
echo ""
echo "No pre-built ONNX model found in repository."
if $HF_CMD download "$MODEL_REPO" --include "model.safetensors" --local-dir "$TEMP_DIR"; then
    echo "Downloaded model.safetensors to $TEMP_DIR"
fi

rm -rf "$TEMP_DIR"
show_manual_instructions
echo ""
echo "After converting, run 'make onnx' again or place the file at: $DEST"
echo ""
echo "After placing the model, fix the tokenizer merges format with:"
echo "  python3 -c \"import json; d=json.load(open('$(dirname \"$DEST\")/tokenizer.json')); d['model']['merges']=[' '.join(m) for m in d['model']['merges']]; json.dump(d, open('$(dirname \"$DEST\")/tokenizer.json','w'), ensure_ascii=False)\""
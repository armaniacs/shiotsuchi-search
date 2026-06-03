# PDF Support

[Japanese](Support-PDF.ja.md)

shiotsuchi-search can index PDF files and make their content searchable. Two extraction methods work together to handle both text-based and scanned PDFs.

## Extraction Methods Overview

| Method | Target | Technology | Feature Flag | Default |
|--------|--------|-----------|-------------|---------|
| **Phase A: Native extraction** | Text-embedded PDFs | pdfium-render + XY-Cut | `pdf` | Enabled |
| **Phase B: VLM extraction** | Scanned PDFs (images only) | edgequake-pdf2md + VLM API | `vlm` | Disabled |

### Processing Flow

```
PDF file
    │
    ▼
Phase A: Text extraction via pdfium-render
    │
    ├── Text found → Index the content
    │
    └── Empty text (scanned PDF)
            │
            ▼
        Phase B: VLM image→text conversion (if enabled)
            │
            ├── Success → Index the content
            └── Failure/not configured → Index metadata only
```

## Phase A: Native Text Extraction

### Overview

Uses [PDFium](https://pdfium.googlesource.com/pdfium/) (the engine embedded in Chrome) via Rust bindings to directly extract text from PDFs.

### Technical Details

1. **Character extraction**: pdfium-render returns each character with its coordinates (x0, y0, x1, y1) and font size
2. **Line clustering** (`cluster_to_lines`): Characters with y-coordinate differences within 0.5× font size are grouped into the same line
3. **XY-Cut layout analysis** (`xycut_to_text`):
   - Column detection: Splits left/right columns using the largest horizontal gap
   - Reading order restoration: Arranges text top-to-bottom, left-to-right
   - Title detection: Lines spanning ≥80% of page width are treated as titles
4. **Markdown conversion**: Heading levels are determined by font size ratios (ratio ≥ 1.5 → H1, ≥ 1.2 → H2)

### Example: Multi-column PDF Processing

```
┌─────────────┬─────────────┐
│  Left col   │  Right col  │
│  body text  │  body text  │
└─────────────┴─────────────┘
```

XY-Cut detects the horizontal gap, processes the left column first, then the right column.

### Configuration

```toml
[indexing]
enable_pdf_extraction = true   # Default: true
include_extensions = ["md", "markdown", "pdf"]  # pdf included by default
```

When `enable_pdf_extraction = false`, PDF files are indexed with empty content (the files themselves are still registered in the DB).

## Phase B: VLM-Based Extraction

### Overview

Uses a Vision Language Model (VLM) to extract text from scanned PDFs (images only). This runs only on PDFs where Phase A produced empty text.

### Supported Providers

| Provider | Example Models | Notes |
|----------|---------------|-------|
| OpenAI | gpt-4.1-nano | High accuracy, paid |
| Anthropic | — | High accuracy, paid |
| Google Gemini | — | High accuracy, paid |
| Ollama | llava, etc. | Local execution, free |

### Configuration

```toml
[vlm]
enabled = true
provider = "openai"           # openai / anthropic / gemini / ollama
model = "gpt-4.1-nano"
max_pages_per_doc = 50        # Omit for unlimited
```

### API Key Setup

```bash
# General setup (recommended)
export SHIOTSUCHI_API_KEY="your-api-key"

# Provider-specific setup
export OPENAI_API_KEY="your-openai-key"
export ANTHROPIC_API_KEY="your-anthropic-key"
```

### Cost Estimates

| Provider | Approximate cost per 50 pages |
|----------|-------------------------------|
| GPT-4.1 | ~$0.40 |
| Amazon Nova Lite | ~$0.01 |
| Ollama (local) | Zero |

## Indexing Mechanism

### Hash-Based Caching

PDF content is tracked using SHA-256 hashes. Re-extraction is skipped when the file hasn't changed.

```
PDF file
    │
    ▼
Text extraction → SHA-256 hash computation
    │
    ├── Hash matches → Skip (use existing index)
    └── Hash differs → Chunk split → Update index
```

### Chunk Splitting

Extracted text is split by the existing Markdown chunker:
- Split on headers (`#`/`##`/`###`)
- Long sections are split on paragraph boundaries
- Each chunk gets an FTS5 entry and optional vector embedding

## Troubleshooting

### PDF Not Indexed

1. Verify `pdf` is in `include_extensions`:
   ```toml
   [indexing]
   include_extensions = ["md", "markdown", "pdf"]
   ```

2. Verify `enable_pdf_extraction` is `true`:
   ```toml
   [indexing]
   enable_pdf_extraction = true
   ```

3. Re-index:
   ```bash
   shiotsuchi index --notes-dir ~/Notes
   ```

### Text Not Extracted Correctly

- Text-based PDFs: Phase A handles these automatically
- Scanned PDFs: Enable Phase B (VLM)
- Mixed PDFs (some text, some images): Phase A extracts text portions, VLM handles the rest

### VLM Extraction Fails

1. Check that the API key is set:
   ```bash
   echo $SHIOTSUCHI_API_KEY
   ```

2. Verify VLM is enabled in config:
   ```toml
   [vlm]
   enabled = true
   ```

3. Check logs for errors:
   ```bash
   RUST_LOG=warn shiotsuchi index --notes-dir ~/Notes
   ```

## Build Options

### Build with PDF Support (Default)

```bash
cargo build --features pdf
```

### Build with VLM Support

```bash
cargo build --features vlm
```

### Build without PDF Support

```bash
cargo build --no-default-features --features watcher,async-index,semantic
```

## Related Documentation

- [ref/architecture.md](../ref/architecture.md) — Architecture overview
- [ref/cli.md](../ref/cli.md) — CLI command reference
- [CHANGELOG.md](../CHANGELOG.md) — Release history

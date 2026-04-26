# LLM Configuration and Model Selection Guide for xcalibre-server

**Model availability and rankings change frequently. These recommendations reflect models available as of early 2026. Check [huggingface.co/models](https://huggingface.co/models) for newer options.**

---

## Table of Contents

1. [Overview](#overview)
2. [Setting Up a Local LLM Server](#setting-up-a-local-llm-server)
3. [Task Requirements and Role Capabilities](#task-requirements-and-role-capabilities)
4. [Model Recommendations by Hardware Tier](#model-recommendations-by-hardware-tier)
5. [Configuration Reference](#configuration-reference)
6. [Tuning System Prompts](#tuning-system-prompts)
7. [Vision LLM Configuration](#vision-llm-configuration)
8. [Semantic Search and Embeddings](#semantic-search-and-embeddings)
9. [Multilingual Libraries](#multilingual-libraries)
10. [Troubleshooting LLM Features](#troubleshooting-llm-features)
11. [Privacy and Security Notes](#privacy-and-security-notes)

---

## Overview

### What LLM Features Do

xcalibre-server's optional AI capabilities enhance library management and enable sophisticated content retrieval. These features are **entirely optional** and disabled by default.

**When enabled, xcalibre-server uses LLMs for:**

1. **Librarian role** — book classification and tagging (genre, subject, reading level), document type detection at ingest
2. **Architect role** — metadata validation and content quality assessment
3. **Synthesis** — cross-document synthesis producing runsheets, design specs, recipes, study guides, compliance summaries, SPICE netlists, KiCad schematics, and 10 other structured output formats from library content
4. **Vision pass** — reading image-heavy PDF pages (schematics, diagrams, charts, assembly drawings) and generating text descriptions for chunking and search
5. **Embeddings** — vector embeddings for semantic search (stored in sqlite-vec)

### Operating Modes

**LLM Disabled (Default)**
- All core library functions work perfectly — browse, search, read, upload, manage metadata
- Only AI-assisted features are unavailable
- No network calls to LLM servers
- Recommended for users who don't need AI features or have no local LLM infrastructure

**LLM Enabled**
- All AI features become available
- Backend queries a local LLM server on book ingest, classification, validation, search, and synthesis operations
- All calls have 10-second timeouts with graceful fallback — failures never block or surface errors to the user

### Privacy: All Data Stays Local

All LLM inference happens on **your own local server**. Book content never leaves your network. xcalibre-server connects exclusively to:
- A local LLM server (LM Studio, Ollama, llama.cpp server) on your LAN or localhost
- Optionally: external metadata APIs (Open Library, Google Books) — these are **not** LLM calls and are entirely optional

### Hardware Requirements Overview

- **Minimal (8GB RAM)**: 3-4B parameter text models, CPU inference (slow but works)
- **Home (16GB VRAM)**: 7-14B models, GPU inference (recommended sweet spot)
- **Workstation (24GB+ VRAM)**: 70B+ models, full precision synthesis quality

Detailed recommendations and expected performance in [Model Recommendations by Hardware Tier](#model-recommendations-by-hardware-tier).

---

## Setting Up a Local LLM Server

xcalibre-server requires an **OpenAI-compatible** local LLM server. Three options below; all are supported.

### LM Studio (Recommended for Beginners)

**Why:** Graphical interface, easiest setup, model downloads built-in, production-ready.

**Setup:**

1. Download from [lmstudio.ai](https://lmstudio.ai)
2. Install and launch LM Studio
3. Go to **Search models** tab, find a recommended model below (e.g., `Phi-3.5-mini-instruct GGUF`)
4. Click download
5. Go to **Local Server** tab → **Start Server**
6. Default URL: `http://localhost:1234/v1`

**GGUF Format Explained**

Models downloaded in LM Studio are in **GGUF** format — a quantized, optimized format for local inference. Quantization reduces model size while maintaining quality:
- **Q4_K_M**: 4-bit quantization, good quality, widely recommended (balances size and accuracy)
- **Q5_K_M**: 5-bit, slightly better quality, larger files
- **Q6_K**: 6-bit, high quality, largest files
- **Q3_K_M**: 3-bit, very small, lower quality (use only on 4GB RAM devices)

**For most users: choose Q4_K_M** — it's the best balance of quality and efficiency.

**Model Auto-Discovery**

If you leave `model = ""` (blank) in `config.toml`, xcalibre-server automatically discovers and uses the first model LM Studio has loaded. You don't need to specify the model name explicitly.

```toml
[llm.librarian]
endpoint = "http://localhost:1234/v1"
model = ""    # xcalibre-server picks the first available model
timeout_secs = 10
system_prompt = "..."
```

### Ollama (Alternative — Great for Headless Servers)

**Why:** Lightweight CLI tool, great for NAS and remote servers without a GUI.

**Setup:**

```bash
# Install (Linux/Mac)
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3.1:8b

# Start the server (runs in background)
ollama serve
```

**Default URL:** `http://localhost:11434/v1`

Unlike LM Studio, Ollama uses the `/v1` path for OpenAI compatibility. Configure:

```toml
[llm.librarian]
endpoint = "http://localhost:11434/v1"
model = ""
timeout_secs = 10
system_prompt = "..."
```

Model management is simpler in Ollama — `ollama list` shows loaded models, `ollama pull modelname` downloads.

### llama.cpp Server (Advanced — Maximum Control)

For users who want the most performance tuning and control over inference parameters.

**Setup:**

```bash
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp && make

./server -m /path/to/model.gguf --port 8000
# Now accessible at http://localhost:8000/v1
```

This is not recommended for beginners — use LM Studio or Ollama unless you need advanced parameter tuning.

---

## Task Requirements and Role Capabilities

Each LLM feature has different demands. Understand what each role needs so you can choose the right model size for your hardware.

### Librarian Role — Classification and Tagging

**Input:** Book title, author, description, first N words of text

**Output:** JSON with genre, tags (array), reading level, document_type

**Requirement:** Must follow JSON instruction reliably; reasoning depth is low — a model that can output structured data accurately

**Good at:** Small to medium models (3B–8B), even on CPU

**Why:** Classification is pattern matching, not deep reasoning. A 3B model trained on fiction/technical categorization is as capable as a 70B model here.

**System Prompt Style:** Structured output with a few examples is highly effective. Include a classification example in your system prompt.

### Architect Role — Metadata Validation and Quality

**Input:** Book metadata fields (title, author, ISBN, description, publication date)

**Output:** JSON with severity level and issues array (e.g., missing ISBN, invalid publication date)

**Requirement:** Light reasoning, reliable JSON output

**Good at:** Small-medium models (3B–8B)

**Why:** Like classification, validation is pattern-based. A model doesn't need 70B parameters to flag missing ISBNs or malformed dates.

### Synthesis — Cross-Document Technical Output

**Input:** Multiple retrieved text chunks from across the library (potentially 10k–30k tokens of context)

**Output:** Structured runsheet / design spec / recipe / compliance summary / SPICE netlist / KiCad schematic / etc. (14 supported formats)

**Requirement:**
- **LONG context window** (32k+ tokens strongly recommended)
- Strong instruction following
- Technical reasoning capability
- **This is the most demanding task** — use your largest available model

**Why:** Synthesis requires understanding multi-document context, reasoning across domains, and producing structured technical output. A 70B model will produce higher-quality runsheets than a 7B model when given the same retrieval and prompt.

**Supported output formats:**
- `runsheet` — procedures, prerequisites, steps, verification, rollback
- `design-spec` — requirements, proposed design, component lists, calculations
- `recipe` — ingredients, method, variations, rationale
- `compliance-summary` — obligations, citations, checklist
- `comparison` — side-by-side analysis
- `study-guide` — key concepts, practice questions
- `cross-reference` — indexed list of locations a term appears
- `research-synthesis` — summarized findings with attribution
- `spice-netlist` — simulation circuit files
- `kicad-schematic` — electronic schematics
- `netlist-json` — structured component/connection data
- `svg-schematic` — rendered schematic images
- `bom` — bill of materials
- `custom` — agent-defined format

### Vision Pass — Image-Heavy PDF Pages

**Input:** Page image (JPEG/PNG) + OCR text

**Output:** Description of schematic/diagram/chart/assembly drawing

**Requirement:** Multimodal model (can see images)

**Triggered when:** Image area > 40% of page AND OCR text < 100 tokens

**Must be:** A vision-capable model; set separately from the text model

**Examples:** Schematics, assembly diagrams, data flow diagrams, waveform plots, layout drawings, infographics

**Gate:** Only runs when `llm.enabled = true` AND the model reports vision capability at startup

### Semantic Search Embeddings

**Input:** Book text chunks

**Output:** Float32 embedding vectors (stored in sqlite-vec)

**Uses:** Dedicated embedding model (not the same as your text generation model)

**Requirement:** Any good embedding model works; consistency is key

**Important:** If you change embedding models, you must re-embed the entire library (`Admin → Jobs → Semantic Reindex All`). Embeddings from different models are incompatible.

---

## Model Recommendations by Hardware Tier

### Tier 1 — Raspberry Pi 4 / 8GB NAS (8GB RAM)

**Constraint:** CPU inference only, expect slow classification (15–60 seconds per book)

**Text Generation Models:**

| Model | HuggingFace | Size | Quantization | Notes |
|---|---|---|---|---|
| **Phi-3.5 Mini Instruct** | `microsoft/Phi-3.5-mini-instruct` | 3.8B | Q4_K_M (~2.4GB) | Best-in-class for size; surprisingly capable; try this first |
| **Llama 3.2 3B Instruct** | `meta-llama/Llama-3.2-3B-Instruct` | 3B | Q4_K_M (~2GB) | Good alternative; solid instruction following |
| **Gemma 2 2B IT** | `google/gemma-2-2b-it` | 2B | Q4_K_M (~1.2GB) | Very fast, decent classification |

**Recommendation:**
- **Librarian:** Phi-3.5 Mini
- **Architect:** Phi-3.5 Mini
- **Synthesis:** Disable (too slow and low quality at 3B)
- **Vision:** Disable (no vision models fit on 8GB)

**Embeddings:** `nomic-ai/nomic-embed-text-v1.5` — 137M params, very fast, good quality

**Config Changes:**
```toml
[llm]
enabled = true

[llm.librarian]
endpoint = "http://localhost:1234/v1"
model = "phi-3.5-mini-instruct-q4_k_m"
timeout_secs = 60      # Allow longer inference on CPU
system_prompt = "..."

[llm.architect]
endpoint = "http://localhost:1234/v1"
model = "phi-3.5-mini-instruct-q4_k_m"
timeout_secs = 60
system_prompt = "..."
```

### Tier 2 — Home Server / NUC / Gaming PC with 8–16GB VRAM (or 32GB RAM)

**Advantage:** GPU inference — much faster (2–10 seconds per classification)

**Text Generation Models:**

| Model | HuggingFace | Size | Quantization | Why This |
|---|---|---|---|---|
| **Llama 3.1 8B Instruct** | `meta-llama/Meta-Llama-3.1-8B-Instruct` | 8B | Q4_K_M (~5GB) | Excellent all-rounder; strong reasoning; production default |
| **Mistral 7B Instruct v0.3** | `mistralai/Mistral-7B-Instruct-v0.3` | 7B | Q4_K_M (~4.2GB) | Strong instruction following; excellent JSON output |
| **Gemma 2 9B IT** | `google/gemma-2-9b-it` | 9B | Q4_K_M (~5.4GB) | Google's best small model; very strong reasoning |
| **Phi-4** | `microsoft/phi-4` | 14B | Q4_K_M (~8.5GB) | Exceptional quality for size; fits in 12GB VRAM; use for synthesis if VRAM allows |
| **Qwen2.5 7B Instruct** | `Qwen/Qwen2.5-7B-Instruct` | 7B | Q4_K_M (~4.2GB) | Excellent multilingual support (important if you have non-English books) |

**Vision Models:**

| Model | HuggingFace | Capability |
|---|---|---|
| **Llama 3.2 Vision 11B** | `meta-llama/Llama-3.2-11B-Vision-Instruct` | First practical vision model for this tier; handles schematics reasonably |
| **LLaVA 1.6 13B** | `llava-hf/llava-1.6-34b-hf` | Reliable, slightly older (consider Qwen2-VL or Llama Vision instead) |

**Recommendation:**

**Librarian:** 7B or 8B model (e.g., Llama 3.1 8B or Mistral 7B)
**Architect:** Same as librarian, or Phi-4 if you want higher quality
**Synthesis:** Phi-4 (14B) if VRAM allows, otherwise librarian's 7B model
**Vision:** Llama 3.2 Vision 11B (if you have image-heavy PDFs)

**Embeddings:**
- `nomic-ai/nomic-embed-text-v1.5` (fast, good quality)
- `BAAI/bge-m3` (multilingual, 570M params)

**Example Config:**
```toml
[llm]
enabled = true
allow_private_endpoints = true    # Allow localhost connection

[llm.librarian]
endpoint = "http://localhost:1234/v1"
model = "llama-3.1-8b-instruct-q4_k_m"
timeout_secs = 30
system_prompt = """
You are a librarian. Classify the book and return JSON only:
{"genre": "...", "tags": ["...", "..."], "reading_level": "adult|ya|children", "document_type": "novel|textbook|reference|magazine|datasheet|comic|audiobook|unknown"}
"""

[llm.architect]
endpoint = "http://localhost:1234/v1"
model = "phi-4-q4_k_m"
timeout_secs = 30
system_prompt = """
You are a metadata validator. Review the book metadata and return JSON only:
{"severity": "ok|warning|error", "issues": [{"field": "...", "severity": "warning|error", "message": "..."}]}
"""
```

### Tier 3 — Workstation with 24GB+ VRAM (RTX 3090 / 4090 / A5000)

**Advantage:** Full GPU inference at speed (sub-second classification); can run largest models

**Text Generation Models:**

| Model | HuggingFace | Size | Notes |
|---|---|---|---|
| **Llama 3.1 70B Instruct** | `meta-llama/Meta-Llama-3.1-70B-Instruct` | 70B | Fits in ~40GB with Q4_K_M; exceptional synthesis quality |
| **Qwen2.5 72B Instruct** | `Qwen/Qwen2.5-72B-Instruct` | 72B | Exceptional multilingual and technical reasoning; best for electronics/legal synthesis |
| **Mistral Large 2** | `mistralai/Mistral-Large-Instruct-2411` | 123B | Requires two 24GB cards or CPU offload; top-tier quality |
| **Phi-4** | `microsoft/phi-4` | 14B | Excellent quality/speed tradeoff; single 24GB card, very fast |

**Vision Models:**

| Model | HuggingFace | Notes |
|---|---|---|
| **Qwen2-VL 7B** | `Qwen/Qwen2-VL-7B-Instruct` | Better than LLaVA for document/schematic understanding |
| **Llama 3.2 Vision 90B** | `meta-llama/Llama-3.2-90B-Vision-Instruct` | Maximum quality (requires ~50GB VRAM) |

**Embedding Models:**

- `BAAI/bge-m3` — best multilingual (570M params)
- `Alibaba-NLP/gte-Qwen2-7B-instruct` — highest quality (7B, needs GPU)

**Recommendation:**

**Librarian:** Phi-4 (fast, excellent quality, fits on single GPU)
**Architect:** Phi-4 or 70B model if you want highest metadata quality
**Synthesis:** 70B model (Llama 3.1 70B or Qwen 72B) — synthesis output is qualitatively much better at 70B than at 7-14B
**Vision:** Qwen2-VL 7B (or Llama 3.2 Vision 90B for absolute best quality)

**Why invest in 70B for synthesis?** At this hardware tier, the quality difference is substantial. A runsheet generated from a 70B model will have better step clarity, more accurate technical reasoning, and fewer hallucinations than a 7B model given the same context.

---

## Configuration Reference

### Basic Structure

All LLM configuration lives in `config.toml`. The `[llm]` section controls:

```toml
[llm]
enabled = true                          # Enable/disable all LLM features
allow_private_endpoints = true          # Required for localhost LM Studio
                                        # Set to false to reject LAN/loopback addresses
```

### Per-Role Configuration

Each role has its own endpoint, model, timeout, and system prompt:

```toml
[llm.librarian]
endpoint = "http://localhost:1234/v1"
model = ""                              # Auto-discover if blank
timeout_secs = 10
system_prompt = """
You are a librarian. Classify the book and return JSON only:
...
"""

[llm.architect]
endpoint = "http://localhost:1234/v1"
model = ""
timeout_secs = 10
system_prompt = """
You are a metadata validator. Return JSON only:
...
"""
```

### Field Descriptions

| Field | Type | Default | Notes |
|---|---|---|---|
| `enabled` | bool | `false` | Master switch for all LLM features |
| `allow_private_endpoints` | bool | `false` | Must be `true` to use LM Studio on localhost/LAN |
| `librarian.endpoint` | string | `` | Full URL to LM server: `http://localhost:1234/v1` or `http://192.168.x.x:1234/v1` |
| `librarian.model` | string | `` | Model identifier. Leave blank to auto-discover. |
| `librarian.timeout_secs` | u64 | 10 | Timeout for inference. CPU inference may need 60+ seconds. |
| `librarian.system_prompt` | string | `` | Full system prompt for classification role |
| `architect.endpoint` | string | `` | (same as librarian) |
| `architect.model` | string | `` | (same as librarian) |
| `architect.timeout_secs` | u64 | 10 | (same as librarian) |
| `architect.system_prompt` | string | `` | (same as librarian) |

### Tier 1 (8GB Pi) Example

```toml
[llm]
enabled = true
allow_private_endpoints = true

[llm.librarian]
endpoint = "http://localhost:1234/v1"
model = "phi-3.5-mini-instruct-q4_k_m"
timeout_secs = 60
system_prompt = """You are a librarian. Classify the book and return JSON only:
{
  "genre": "fiction|nonfiction|reference|technical",
  "tags": ["tag1", "tag2"],
  "reading_level": "children|ya|adult",
  "document_type": "novel|textbook|reference|magazine|datasheet|comic|audiobook|unknown"
}"""

[llm.architect]
endpoint = "http://localhost:1234/v1"
model = "phi-3.5-mini-instruct-q4_k_m"
timeout_secs = 60
system_prompt = """You are a metadata validator. Review the metadata and return JSON only:
{
  "severity": "ok|warning|error",
  "issues": [
    {
      "field": "title|author|isbn",
      "severity": "warning|error",
      "message": "...",
      "suggestion": "..."
    }
  ]
}"""
```

### Tier 2 (16GB VRAM) Example

```toml
[llm]
enabled = true
allow_private_endpoints = true

[llm.librarian]
endpoint = "http://localhost:1234/v1"
model = "llama-3.1-8b-instruct-q4_k_m"
timeout_secs = 30
system_prompt = """You are a librarian. Classify the book accurately.
Return JSON only: {"genre": "...", "tags": [...], "reading_level": "...", "document_type": "..."}

Example:
Input: "The Great Gatsby" by F. Scott Fitzgerald
Output: {"genre": "literary-fiction", "tags": ["1920s", "american", "classic", "romance"], "reading_level": "adult", "document_type": "novel"}
"""

[llm.architect]
endpoint = "http://localhost:1234/v1"
model = "phi-4-q4_k_m"
timeout_secs = 30
system_prompt = """You are a metadata expert. Validate book metadata.
Return JSON only: {"severity": "ok|warning|error", "issues": [...]}"""
```

---

## Tuning System Prompts

xcalibre-server's system prompts are fully configurable in `config.toml` without code changes. The prompt eval framework lets you test prompts before promoting them to production.

### Using the Prompt Eval Framework

The eval framework is built into xcalibre-server. Write a fixture (test case), run evals against models, and promote prompts once they pass.

**Run all evaluations against the librarian model:**
```bash
docker exec xcalibre-server xcalibre-server eval --role librarian
```

**Run against a specific model:**
```bash
docker exec xcalibre-server xcalibre-server eval --role librarian --model phi-4
```

**Run against multiple models and compare:**
```bash
docker exec xcalibre-server xcalibre-server eval --role librarian --model phi-3.5-mini --model llama-3.1-8b
```

**Run a single fixture:**
```bash
docker exec xcalibre-server xcalibre-server eval --fixture classify_fiction
```

Results are shown in **Admin Panel → Prompt Evals** as a pass/fail matrix per model per fixture.

### Writing a Fixture

Create fixture files in `evals/fixtures/`. Each is a TOML file describing a test case:

```toml
# evals/fixtures/classify_fiction.toml
name = "classify_fiction"
role = "librarian"
description = "Should classify a clearly fictional novel correctly"

[input]
title = "The Great Gatsby"
author = "F. Scott Fitzgerald"
description = "A story of the fabulously wealthy Jay Gatsby and his love for Daisy Buchanan."

[[expect]]
type = "json_valid"                     # Must parse as JSON

[[expect]]
type = "contains_field"
field = "genre"                         # JSON must have a "genre" key

[[expect]]
type = "field_matches"
field = "genre"
pattern = "(?i)fiction|literary"        # Case-insensitive regex match

[[expect]]
type = "array_min_length"
field = "tags"
min = 2                                 # At least 2 tags

[[expect]]
type = "latency_ms"
max = 15000                             # Must respond within 15s
```

### Tips for Better Classification Prompts

1. **Always ask for JSON-only output** — models that produce prose around JSON are unreliable
2. **Include a few-shot example** — even one example dramatically improves accuracy
3. **Use the `document_type` field** to distinguish novels from technical references — this improves RAG retrieval precision
4. **Check eval results first** — if classification quality is poor, the eval results often pinpoint which fixtures (which genres/formats) are failing
5. **Test iteratively** — tune prompts against fixtures before deploying to production

### Example: Improved Librarian Prompt

Before:
```
You are a librarian. Classify books by genre and reading level.
```

After (with example):
```
You are a librarian. Classify the book accurately. Return JSON only.

Example:
Input: "The Great Gatsby" by F. Scott Fitzgerald, description: "A story of wealth and love in 1920s America"
Output:
{
  "genre": "literary-fiction",
  "tags": ["1920s", "american", "classic", "romance", "social-commentary"],
  "reading_level": "adult",
  "document_type": "novel"
}

Now classify this book:
```

---

## Vision LLM Configuration

Vision support is enabled automatically when:
1. `llm.enabled = true`
2. The configured model reports vision capability at startup

xcalibre-server queries `/v1/models` on startup to detect this. If your primary model isn't vision-capable, you can load a separate vision model in LM Studio and point a different endpoint at it.

### Which Documents Trigger the Vision Pass

Image-heavy pages in PDFs and EPUBs are processed by the vision LLM when:
- **Image area > 40% of page** AND
- **OCR text < 100 tokens**

Typical triggers: circuit schematics, assembly drawings, wiring diagrams, data flow diagrams, floor plans, waveform plots, infographics

### Best Vision Models for Technical Content

| Model | HuggingFace | Best For |
|---|---|---|
| **Qwen2-VL 7B** | `Qwen/Qwen2-VL-7B-Instruct` | Reading component labels, net names, reference designators on schematics |
| **Llama 3.2 Vision 11B** | `meta-llama/Llama-3.2-11B-Vision-Instruct` | General-purpose; weaker on dense technical diagrams than Qwen2-VL |
| **LLaVA 1.6 13B** | `llava-hf/llava-1.6-13b-hf` | Reliable but older; Qwen2-VL generally better for technical content |

### Using a Separate Vision Model

If your primary model isn't vision-capable (e.g., you're running Phi-4 for synthesis but need vision for schematics):

1. In LM Studio: Load the vision model on a different port (e.g., 1235)
2. In `config.toml`: Add a `[llm.vision]` section (when/if implemented) pointing to that port
3. Otherwise: Load the vision model on the same port and let it be discovered first — xcalibre-server will use it

---

## Semantic Search and Embeddings

Semantic search finds books by **meaning** rather than keywords. It requires dedicated embedding models — **not the same as your text generation model**.

### Setting Up Embeddings

1. **In LM Studio:** Load an embedding model (e.g., `nomic-embed-text-v1.5`)
2. **In `config.toml`:**
   ```toml
   [llm.embeddings]
   endpoint = "http://localhost:1234/v1"
   model = "nomic-embed-text-v1.5"
   ```
3. **In Admin Panel:** Go to **Jobs → Queue semantic reindex** to embed your entire library

### Recommended Embedding Models

| Model | HuggingFace | Size | Best For |
|---|---|---|---|
| **nomic-embed-text-v1.5** | `nomic-ai/nomic-embed-text-v1.5` | 137M | General use, fast, good quality |
| **bge-m3** | `BAAI/bge-m3` | 570M | Multilingual libraries (supports 100+ languages) |
| **bge-large-en-v1.5** | `BAAI/bge-large-en-v1.5` | 335M | English-only, higher quality than nomic |
| **gte-Qwen2-7B-instruct** | `Alibaba-NLP/gte-Qwen2-7B-instruct` | 7B | Highest quality (needs GPU); slow but excellent precision |

### Important: Changing Embedding Models

If you change embedding models, **you must re-embed the entire library**:

1. Admin Panel → **Jobs → Semantic Reindex All**
2. This queues a background job to re-embed every book chunk
3. Old embeddings are **not compatible** with new models — they must be replaced

---

## Multilingual Libraries

If your library contains non-English books, choose models with strong multilingual support.

### Best Models for Multilingual Support

**Text Generation:**
- **Qwen2.5 series** (any size) — strongest multilingual support, especially for Asian languages
- **Gemma 2** models — strong European language support
- **Mistral 7B/Large** — good European language support

**Embeddings:**
- **BAAI/bge-m3** — explicitly designed for multilingual retrieval; handles 100+ languages
- **nomic-embed-text-v1.5** — supports multiple languages (check docs for full list)

**Vision (if needed):**
- **Qwen2-VL** — multilingual image understanding

### BCP 47 Language Codes

Set the book's `language` field to BCP 47 codes for best search filtering:
- `en` — English
- `fr` — French
- `de` — German
- `es` — Spanish
- `zh` — Chinese (Simplified)
- `zh-Hant` — Chinese (Traditional)
- `ja` — Japanese
- `ko` — Korean

### Example: French Library with Qwen2.5 and bge-m3

```toml
[llm]
enabled = true

[llm.librarian]
endpoint = "http://localhost:1234/v1"
model = "qwen2.5-7b-instruct-q4_k_m"
system_prompt = """Tu es un bibliothécaire. Classe le livre et retourne du JSON uniquement:
{"genre": "...", "tags": [...], "reading_level": "...", "document_type": "..."}"""

[llm.embeddings]
endpoint = "http://localhost:1234/v1"
model = "bge-m3"
```

---

## Troubleshooting LLM Features

| Problem | Likely Cause | Fix |
|---|---|---|
| "LLM unavailable" everywhere | `llm.enabled = false` in config | Set `enabled = true` and restart |
| Classification produces nonsense | Model not following JSON format | Add stricter JSON-only instruction to system prompt; check eval results |
| Semantic search returns irrelevant results | Wrong embedding model or needs reindex | Go to Admin → Jobs → Semantic Reindex All |
| Vision pass not running | Model not vision-capable or feature not gated properly | Load a vision model; check startup log for "vision capability: false" |
| Synthesis very slow | Large context + small model | Use a larger model for synthesis; consider using Tier 3 hardware for synthesis-heavy workloads |
| Timeout errors | Model too slow for 10s default | Increase `timeout_secs` in config for the affected role (CPU inference may need 60+ seconds) |
| LLM endpoint connection refused | Endpoint not running or wrong URL | Verify LM Studio/Ollama is running; check `endpoint` URL in config.toml |
| Empty `model = ""` not auto-discovering | Server doesn't report available models | Manually specify the model name instead, or restart LM Studio |

---

## Privacy and Security Notes

### Data Never Leaves Your Network

All LLM inference happens on **your local server**. Book content is never sent to:
- OpenAI
- Anthropic
- Google
- Any external service

The only network calls xcalibre-server makes are to:
- Your local LLM server (LM Studio, Ollama, etc.) on your LAN or localhost
- Optionally: external metadata APIs (Open Library, Google Books) for book lookup — these are **not** LLM calls and are entirely optional

### Private LAN Servers

If you're running LM Studio on another machine on your local network (e.g., a NAS running LM Studio at `192.168.1.100:1234`):

```toml
[llm]
allow_private_endpoints = true   # Required to allow RFC 1918 addresses

[llm.librarian]
endpoint = "http://192.168.1.100:1234/v1"
```

**Note:** By default, `allow_private_endpoints = false`. This is a security guard to prevent SSRF injection. Set it to `true` only if you're intentionally using a LAN-hosted LLM server.

### Startup Validation

When xcalibre-server starts, it validates all configured LLM endpoints:
- Checks that endpoints are valid URLs
- Warns (but does not block) if an endpoint points to a private/loopback address and `allow_private_endpoints = false`
- Intentionally allows localhost for single-machine deployments

### LLM Endpoints are Config-File Only

LLM endpoints are configured in `config.toml` at startup. They **cannot be changed at runtime via API**, preventing SSRF injection attacks through the admin panel. If you need to change an endpoint, update the config file and restart.

### Synthesis Prompt Injection Prevention

When running synthesis tasks, user-supplied prompts are fenced inside SOURCE delimiters to prevent prompt injection attacks:

```
You have access to the following sources:
[SOURCE]
<retrieved content here>
[/SOURCE]

Now answer: <user prompt here>
```

This prevents a user from escaping the retrieval context or manipulating the model into ignoring source attribution.

---

**Last Updated:** 2026-04-24

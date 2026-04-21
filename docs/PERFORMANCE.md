# Performance Targets

## Hardware Reference: Raspberry Pi 4B (4-core Cortex-A72, 1.5GHz, 4GB RAM)

These targets apply to a library of ~1000 books under 1-5 concurrent users.
All measurements are p99 latency under light concurrent load.

| Operation | Target | Notes |
|---|---|---|
| GET /books (page 30, no filter) | < 50ms | FTS5 disabled - pure SQL |
| GET /books?q=search | < 100ms | FTS5 MATCH query |
| GET /books/:id | < 20ms | Joins: authors, tags, formats, identifiers |
| POST /books (upload, no LLM) | < 500ms | Cover extraction + DB write + meili index |
| GET /books/:id/text (EPUB chapter) | < 200ms | EPUB unzip + HTML strip |
| GET /search?mode=semantic | < 300ms | sqlite-vec ANN search, 1000 embeddings |
| Memory at idle | < 64MB RSS | After startup, no active requests |
| Memory under load | < 128MB RSS | 5 concurrent users, mix of reads |

## How to Benchmark Locally

```bash
cargo bench --bench api_benchmarks 2>&1 | grep "time:"
```

## Notes on ARM Cross-Compilation

The arm64 and armv7 images are cross-compiled on amd64 CI runners using
QEMU emulation for the final stages. Performance of the cross-compiled binary
is equivalent to native - cross-compilation only affects build time, not runtime.

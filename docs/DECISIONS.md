# Architecture Decisions

## ADR-001 — Two-model development pipeline
**Date**: 2026-04-16
**Status**: Active

**Decision**: Use Nemotron-120B as architect/reviewer and Qwen2.5-Coder-7B as coder, orchestrated by a Python script.

**Reasoning**:
- Large models (Nemotron) produce better plans and catch more issues in review, but are slow for code generation
- Small code-specialized models (Qwen) generate code faster and follow format instructions more reliably
- Separating roles lets each model do what it does best

**Tradeoffs**:
- Adds orchestration complexity vs. single-model approach
- Nemotron's 120B size means slower planning steps (~30–60s per call)
- Qwen-7B may miss subtle architectural issues Nemotron would catch

---

## ADR-002 — Separate librarian endpoint
**Date**: 2026-04-16
**Status**: Active

**Decision**: Run the runtime librarian model (Phi-3-mini) on a separate machine (`192.168.0.72:1234`) from the development models.

**Reasoning**:
- Phi-3-mini, Nemotron-120B, and Qwen-7B cannot all run simultaneously on one machine without memory pressure affecting quality
- Separate machine means librarian is always available independent of dev model state
- Mirrors production topology — calibre-web calling a dedicated inference server

**Tradeoffs**:
- Network dependency — features degrade gracefully if `192.168.0.72` is unreachable (handled by `ENABLE_LLM_FEATURES` flag)
- Must keep LM Studio running on both machines during development and use

---

## ADR-003 — Phi-3-mini as librarian model
**Date**: 2026-04-16
**Status**: Active

**Decision**: Use `phi-3-mini-4k-instruct` for runtime classification, tagging, and metadata tasks inside calibre-web.

**Reasoning**:
- Already available on the remote machine — no additional download
- 3.8B parameters fits easily alongside other workloads
- Strong structured output and classification performance for its size
- Fast inference — acceptable latency for web UI interactions

**Tradeoffs**:
- 4K context window is limited — long documents must be chunked
- Not as capable as larger models for nuanced literary classification
- May require prompt tuning for domain-specific tagging accuracy

---

## ADR-004 — ENABLE_LLM_FEATURES flag (default False)
**Date**: 2026-04-16
**Status**: Active

**Decision**: All AI features are gated behind a single config flag, disabled by default.

**Reasoning**:
- calibre-web is a general-purpose tool — AI features are an enhancement, not a requirement
- Users without LM Studio should have a fully functional experience
- Default-off prevents unexpected network calls or errors on first run

**Tradeoffs**:
- Extra configuration step for users who want AI features
- All new code must check this flag — adds boilerplate to every AI-adjacent route

---

## ADR-005 — One feature branch per feature
**Date**: 2026-04-16
**Status**: Active

**Decision**: Each of the 8 features gets its own `feature/<name>` git branch, merged to `lmstudio-agent` when complete.

**Reasoning**:
- Isolates failures — a broken feature doesn't affect others in development
- Allows independent review of each feature's diff
- Matches the orchestrator's task-per-session model

**Tradeoffs**:
- More merge operations
- Features with shared infrastructure (e.g. LLM client wrapper) must be in the first branch (graceful degradation) before others start

---

## ADR-006 — MCP server wrapping the orchestrator
**Date**: 2026-04-16
**Status**: Active

**Decision**: Expose the orchestrator via an MCP server so opencode can call it as a tool rather than requiring the user to switch between interfaces.

**Reasoning**:
- opencode is already the primary interface — keeping everything there reduces context switching
- MCP tools are first-class in opencode — model can decide when to invoke them
- Node.js MCP server calling Python subprocess avoids Python 3.10+ requirement for the MCP SDK while keeping orchestrator logic in Python

**Tradeoffs**:
- Two-process architecture (Node → Python) adds a failure point
- Long-running orchestrator calls block the MCP tool response — no streaming progress
- If orchestrator hangs, MCP tool hangs silently

---

## ADR-007 — Python 3.14 for development venv
**Date**: 2026-04-16
**Status**: Active

**Decision**: Rebuild the calibre-web development venv on Python 3.14 (Homebrew) instead of the original 3.9.

**Reasoning**:
- All 5 vulnerable packages (filelock, requests, pytest, pypdf, setuptools) had patched versions that dropped Python 3.9 support
- Python 3.9 reached end-of-life October 2025
- Python 3.14 is available via Homebrew on the M4 Mac Studio

**Tradeoffs**:
- calibre-web's `requirements.txt` pins `iso-639` for `python_version<'3.12'` — replaced by `pycountry` for 3.12+ (already handled in requirements.txt conditionally)
- Production deployments may still use older Python — dev/prod parity is not guaranteed
- Must retest that calibre-web actually runs on 3.14 before any features are merged

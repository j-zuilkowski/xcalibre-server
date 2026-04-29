# Codex Task: Fix Phase 10 Stage 5 code review issues

## Objective

Seven targeted fixes across the backend and web frontend identified in a post-commit code review of
Phase 10 Stage 5 (commit c8a1768). No new features — only correctness, maintainability, and
reliability improvements. All existing tests must continue to pass.

## Context

- Language/Framework: Rust 2021 + Axum 0.7 (backend), React + Vite + TypeScript (web frontend)
- Repo root: `/path/to/xcalibre-server`
- Key files:
  - `backend/src/api/books.rs`
  - `backend/src/ingest/text.rs`
  - `backend/src/ingest/mod.rs`
  - `apps/web/src/features/reader/DjvuReader.tsx`
  - `apps/web/src/features/reader/AudioReader.tsx`
- Do NOT touch: migrations, schema, auth, any file not listed under a requirement below
- Constraints:
  - `cargo clippy --workspace -- -D warnings` must pass at zero warnings after every Rust change
  - `pnpm --filter @xs/web build` must succeed after every TypeScript change
  - No `unwrap()` in production Rust code — use `?` or `.unwrap_or_default()`

---

## Stage 1 — Backend fixes (Rust)

### Requirement 1 — Extract shared MOBI helpers into `backend/src/ingest/mobi_util.rs`

**Problem:** The following functions exist in nearly identical form in BOTH `backend/src/api/books.rs`
AND `backend/src/ingest/text.rs`:

| Function in `books.rs` | Function in `text.rs` |
|---|---|
| `split_on_mobi_pagebreaks` | `split_on_mobi_pagebreak` (singular) |
| `split_on_heading_tags` | `split_on_heading_tags` |
| `find_next_heading_index` | `find_next_heading_index` |
| `safe_mobi_content` | `safe_mobi_content` |
| `extract_heading_title` | `extract_chapter_title` |
| `strip_html_fragment_to_text` | `strip_html_to_text` |
| `decode_basic_html_entities` | (inline inside `strip_html_to_text`) |

**Fix:**

1. Create `backend/src/ingest/mobi_util.rs` with canonical public versions of all these helpers.
   Use the `books.rs` versions as the source of truth (they are more complete), but adopt the
   `text.rs` single-form name `split_on_mobi_pagebreak` (not `split_on_mobi_pagebreaks`).
   Exported symbols:
   - `pub fn split_on_mobi_pagebreak(raw_html: &str) -> Vec<String>`
   - `pub fn split_on_heading_tags(raw_html: &str) -> Vec<String>`
   - `pub fn find_next_heading_index(lower: &str, cursor: usize) -> Option<usize>`
   - `pub fn safe_mobi_content(book: &mobi::Mobi) -> String`
   - `pub fn extract_heading_title(segment: &str) -> Option<String>`
   - `pub fn strip_html_to_text(fragment: &str) -> String` (rename of `strip_html_fragment_to_text`)
   - `pub fn decode_basic_html_entities(value: &str) -> String`
   - `pub fn xml_escape(value: &str) -> String` (move from `books.rs`)

2. In `backend/src/ingest/mod.rs`, add: `pub mod mobi_util;`

3. In `backend/src/api/books.rs`, remove the duplicated private functions and replace all call
   sites with `crate::ingest::mobi_util::*` or individual imports.
   - `build_epub_from_mobi` uses: `safe_mobi_content`, `split_on_mobi_pagebreak`,
     `split_on_heading_tags`, `extract_heading_title`, `strip_html_to_text`, `xml_escape`
   - `sanitize_file_name_for_header` stays in `books.rs` (it is HTTP-specific, not ingest logic)

4. In `backend/src/ingest/text.rs`, remove the private duplicates and import from `mobi_util`:
   - Remove: `split_on_mobi_pagebreak`, `split_on_heading_tags`, `find_next_heading_index`,
     `safe_mobi_content`, `extract_chapter_title` (now `extract_heading_title`), `strip_html_to_text`,
     `decode_basic_html_entities`
   - Add at top: `use super::mobi_util::{…};`
   - Update all call sites in `text.rs` to match the new canonical names.

---

### Requirement 2 — Fix EPUB unique identifier in `build_epub_from_mobi`

**File:** `backend/src/api/books.rs`, function `mobi_to_epub`

**Problem:** The EPUB `<dc:identifier>` and both `dtb:uid` meta elements use
`sanitize_file_name_for_header(&title)` as the unique ID. This is not stable and not unique.

**Fix:**

`mobi_to_epub` already receives `book_id: String` via `Path((book_id, format))`. Pass `book_id` as
a parameter into `build_epub_from_mobi`, or compute a stable ID before calling it.

In `build_epub_from_mobi` (or its call site), replace every occurrence of
`sanitize_file_name_for_header(&title)` used as a UID with the actual `book_id` string. The
filename in `Content-Disposition` can continue using `sanitize_file_name_for_header(&title)`.

Concretely, in `content.opf`:
```xml
<!-- BEFORE -->
<dc:identifier id="bookid">urn:xcalibre-server:{sanitize_file_name_for_header(&title)}</dc:identifier>
<!-- AFTER -->
<dc:identifier id="bookid">urn:xcalibre-server:{book_id}</dc:identifier>
```

Same change in `toc.ncx` `dtb:uid` meta content.

---

### Requirement 3 — Preserve paragraph structure in EPUB chapter body

**File:** `backend/src/api/books.rs`, function `build_epub_from_mobi` / `strip_html_to_text`
(or `strip_html_fragment_to_text` before the rename in Req 1).

**Problem:** The EPUB chapter XHTML currently renders as a single `<p>{chapter_body}</p>`, collapsing
all paragraph structure from the original MOBI HTML.

**Fix:** In `mobi_util::strip_html_to_text` (the shared version from Req 1), when a `</p>`, `</div>`,
`<br>`, or `<br/>` closing/self-closing tag is encountered during HTML stripping, emit `\n\n` into the
output buffer instead of a space, so paragraph breaks are preserved.

Then in `build_epub_from_mobi`, replace the single `<p>{chapter_body}</p>` with multi-paragraph XHTML
by splitting the stripped text on `\n\n` and wrapping each non-empty segment in its own `<p>…</p>`.

```rust
let paragraphs = chapter.text
    .split("\n\n")
    .map(|s| s.trim())
    .filter(|s| !s.is_empty())
    .map(|s| format!("<p>{}</p>", xml_escape(s)))
    .collect::<Vec<_>>()
    .join("\n    ");
// then use paragraphs in the chapter XHTML body instead of a single <p>
```

---

### Requirement 4 — Move `guessed_mime` computation inside the fallback arm

**File:** `backend/src/api/books.rs`, function `stream_format`

**Problem:** `guessed_mime` is computed unconditionally before the `match` but is only used in the
`_ =>` arm.

**Fix:** Inline the `mime_guess` call into the `_ =>` arm and remove the variable from the outer scope:

```rust
let content_type = match file_extension.as_str() {
    "mp3" => "audio/mpeg",
    "m4b" | "m4a" => "audio/mp4",
    "ogg" | "opus" => "audio/ogg",
    "flac" => "audio/flac",
    _ => {
        // leak is fine for static-lifetime mime strings; or store in a local let
        let guessed = mime_guess::from_ext(&file_extension).first_or_octet_stream();
        guessed.essence_str() // NOTE: this borrows guessed, so bind it first
    }
};
```

Because `essence_str()` returns a `&str` borrowed from the `Mime` value, bind `guessed` as a `let`
before the `match` only for the fallback case, or change `content_type` to `String`. The simplest
fix: keep the `let guessed_mime` but move it inside `_ => { let guessed_mime = …; guessed_mime.essence_str() }`.

---

## Stage 2 — Frontend fixes (TypeScript / React)

### Requirement 5 — Vendor djvu.js instead of loading from CDN

**File:** `apps/web/src/features/reader/DjvuReader.tsx`

**Problem:** `loadDjvuModule()` fetches from `https://cdn.jsdelivr.net/npm/djvu.js@0.3.2/…` at
runtime. This breaks in air-gapped / self-hosted deployments — the primary use case for xcalibre-server.

**Fix:**

1. Download the UMD bundle and save it to `apps/web/public/djvu.min.js`:
   ```
   curl -fsSL "https://cdn.jsdelivr.net/npm/djvu.js@0.3.2/dist/djvu.min.js" \
     -o apps/web/public/djvu.min.js
   ```

2. In `DjvuReader.tsx`, change `loadDjvuModule` to import from the local public path:
   ```ts
   async function loadDjvuModule(): Promise<{ App?: DjvuAppCtor }> {
     const fallback = (await import(
       /* @vite-ignore */ "/djvu.min.js"
     )) as Record<string, unknown>;
     const root = (fallback.default ?? fallback) as Record<string, unknown>;
     const nested = (root.DjVu ?? root.djvu ?? root) as Record<string, unknown>;
     return { App: nested.App as DjvuAppCtor | undefined };
   }
   ```

3. Add `apps/web/public/djvu.min.js` to `.gitignore` (it is a vendored binary artifact).
   Add a comment in `DjvuReader.tsx` above `loadDjvuModule` noting the vendor location and
   the curl command to re-download it.

---

### Requirement 6 — Fix audio progress: add periodic flush during continuous playback

**File:** `apps/web/src/features/reader/AudioReader.tsx`

**Problem:** `handleProgressChange` in `ReaderPage.tsx` uses a 600ms debounce that resets on every
call. During continuous audio playback, `timeupdate` fires every ~250ms, which means the debounce
timer is perpetually reset and progress is **never actually saved** until playback stops. Long
playback sessions would lose position entirely if the page is closed mid-session.

**Fix:** Add a periodic flush inside `AudioReader` using `setInterval` (every 30 seconds) that calls
`onProgressChange` with the current position regardless of the debounce in the parent. This ensures
progress is written at most once per 30s during active playback.

```tsx
useEffect(() => {
  const id = window.setInterval(() => {
    const audio = audioRef.current;
    if (!audio || audio.paused || !Number.isFinite(audio.duration) || audio.duration <= 0) {
      return;
    }
    const percentage = clampPercentage((audio.currentTime / audio.duration) * 100);
    onProgressChange({ percentage, cfi: null, page: Math.floor(audio.currentTime) });
  }, 30_000);

  return () => window.clearInterval(id);
}, [onProgressChange]);
```

Keep the existing `onTimeUpdate` handler unchanged (it drives the parent debounce for seek/pause
events). The periodic flush is additive.

---

### Requirement 7 — Add console.warn on DJVU page render retry

**File:** `apps/web/src/features/reader/DjvuReader.tsx`, function `renderPage`

**Problem:** When a page render fails, the code silently retries page-1 with no log output, making
failures impossible to diagnose.

**Fix:** Add a `console.warn` before the fallback attempt:

```ts
} catch (err) {
  if (safePage > 1) {
    console.warn(`[DjvuReader] page ${safePage} render failed, retrying page ${safePage - 1}`, err);
    await renderWithBestEffort(app, canvas, safePage - 1);
  } else {
    throw new Error("djvu page render failed");
  }
}
```

---

## Acceptance Criteria

- [ ] `cargo clippy --workspace -- -D warnings` passes with zero warnings
- [ ] `cargo test --workspace` passes with all existing tests green
- [ ] `pnpm --filter @xs/web build` succeeds with zero errors
- [ ] `backend/src/ingest/mobi_util.rs` exists and is declared in `mod.rs`
- [ ] No duplicate mobi helper functions remain in `books.rs` or `text.rs`
- [ ] `build_epub_from_mobi` uses `book_id` (not a sanitized title) in `dc:identifier` and `dtb:uid`
- [ ] EPUB chapter XHTML contains multiple `<p>` tags for multi-paragraph content
- [ ] `guessed_mime` computation is inside the `_ =>` arm of the `stream_format` match
- [ ] `apps/web/public/djvu.min.js` exists (vendored)
- [ ] `DjvuReader.tsx` imports from `/djvu.min.js` not a CDN URL
- [ ] `AudioReader.tsx` has a 30-second `setInterval` periodic progress flush
- [ ] `DjvuReader.tsx` `renderPage` logs a `console.warn` before the fallback retry

## Additional Notes

- Run Stage 1 and Stage 2 as separate Codex invocations with a `git diff` + test checkpoint between them.
- The `mobi_util.rs` extraction is the most complex change — clippy will catch any unused imports
  left in `books.rs` or `text.rs` after the refactor.
- `essence_str()` returns a `&str` borrowed from the `Mime` value — make sure the `Mime` is bound
  in a `let` that outlives the borrow when restructuring the `stream_format` match.
- The `djvu.min.js` file should NOT be committed to git (add to `.gitignore`). Codex should emit
  the curl command and the gitignore entry.

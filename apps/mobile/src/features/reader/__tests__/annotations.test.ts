/**
 * Unit tests for the pure annotation helper functions in `annotations.ts`.
 *
 * All helpers are pure functions with no side effects, so each test creates
 * fresh annotation objects using the `makeAnnotation` factory without needing
 * database setup or mock teardown.
 *
 * Covered behaviors:
 * - `upsertAnnotation`: inserts and sorts by CFI, then by created_at
 * - `upsertAnnotation`: replaces an existing entry matched by id
 * - `removeAnnotation`: drops the annotation with the matching id
 * - `annotationPreviewText`: priority order (note > highlighted_text > cfi_range)
 * - `updateAnnotationColor` + `annotationColorPatch`: immutable color update
 * - `updateAnnotationNote` + `annotationNotePatch`: trimming and empty-string → null
 */
import type { BookAnnotation } from "@xs/shared";
import {
  annotationColorPatch,
  annotationNotePatch,
  annotationPreviewText,
  removeAnnotation,
  sortAnnotations,
  updateAnnotationColor,
  updateAnnotationNote,
  upsertAnnotation,
} from "../annotations";

/**
 * Factory that creates a fully-typed `BookAnnotation` with sensible defaults.
 * Overrides are merged on top of the defaults so tests only specify what varies.
 */
function makeAnnotation(overrides: Partial<BookAnnotation>): BookAnnotation {
  return {
    id: "annotation-1",
    user_id: "user-1",
    book_id: "book-1",
    type: "highlight",
    cfi_range: "epubcfi(/6/2[chapter-1]!/4/2/2:0)",
    highlighted_text: "Selected text",
    note: null,
    color: "yellow",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

/** Test suite for annotation list management helpers. */
describe("reader annotations helpers", () => {
  /**
   * Verifies that `upsertAnnotation` inserts a new annotation and the resulting
   * list is sorted first by CFI range (document order) and then by created_at
   * when two annotations share the same CFI range.
   */
  test("test_upsert_annotation_inserts_and_sorts_by_cfi_then_created_at", () => {
    const first = makeAnnotation({
      id: "b",
      cfi_range: "epubcfi(/6/2[chapter-2]!/4/2/2:0)",
      created_at: "2026-01-02T00:00:00Z",
    });
    const second = makeAnnotation({
      id: "a",
      cfi_range: "epubcfi(/6/2[chapter-1]!/4/2/2:0)",
      created_at: "2026-01-03T00:00:00Z",
    });
    const third = makeAnnotation({
      id: "c",
      cfi_range: "epubcfi(/6/2[chapter-1]!/4/2/2:0)",
      created_at: "2026-01-01T00:00:00Z",
    });

    const next = upsertAnnotation([first, second], third);

    expect(next.map((entry) => entry.id)).toEqual(["c", "a", "b"]);
  });

  /**
   * Verifies that `upsertAnnotation` replaces an existing annotation when a new
   * annotation with the same `id` is provided, leaving the list length unchanged.
   */
  test("test_upsert_annotation_replaces_existing_entry", () => {
    const original = makeAnnotation({ id: "annotation-1", color: "yellow" });
    const updated = makeAnnotation({ id: "annotation-1", color: "blue" });

    const next = upsertAnnotation([original], updated);

    expect(next).toHaveLength(1);
    expect(next[0]).toEqual(updated);
  });

  /**
   * Verifies that `removeAnnotation` removes only the annotation whose `id` matches
   * and leaves the remaining annotations untouched.
   */
  test("test_remove_annotation_drops_matching_id", () => {
    const annotations = [
      makeAnnotation({ id: "annotation-1" }),
      makeAnnotation({ id: "annotation-2" }),
    ];

    expect(removeAnnotation(annotations, "annotation-1").map((entry) => entry.id)).toEqual(["annotation-2"]);
  });

  /**
   * Verifies the priority order of `annotationPreviewText`:
   * 1. Trimmed `note` (when non-empty)
   * 2. Trimmed `highlighted_text` (when note is absent)
   * 3. `cfi_range` fallback (when both note and highlighted_text are null)
   */
  test("test_annotation_preview_prefers_note_then_highlighted_text_then_cfi", () => {
    expect(
      annotationPreviewText(
        makeAnnotation({
          note: "  Note first  ",
          highlighted_text: "Selected text",
        }),
      ),
    ).toBe("Note first");

    expect(
      annotationPreviewText(
        makeAnnotation({
          note: null,
          highlighted_text: "  Selected text  ",
        }),
      ),
    ).toBe("Selected text");

    expect(
      annotationPreviewText(
        makeAnnotation({
          note: null,
          highlighted_text: null,
        }),
      ),
    ).toBe("epubcfi(/6/2[chapter-1]!/4/2/2:0)");
  });

  /**
   * Verifies that `updateAnnotationColor` creates a shallow copy with only the
   * color changed (all other fields preserved), and that `annotationColorPatch`
   * produces the correct PATCH request shape.
   */
  test("test_color_patch_logic_preserves_other_fields", () => {
    const annotation = makeAnnotation({
      color: "yellow",
      note: "Existing note",
    });

    const updated = updateAnnotationColor(annotation, "pink");

    expect(updated).toEqual({
      ...annotation,
      color: "pink",
    });
    expect(annotationColorPatch("pink")).toEqual({ color: "pink" });
  });

  test("test_note_patch_trims_whitespace_and_clears_empty_values", () => {
    const annotation = makeAnnotation({
      note: "Existing note",
    });

    expect(updateAnnotationNote(annotation, "  Updated note  ")).toEqual({
      ...annotation,
      note: "Updated note",
    });
    expect(annotationNotePatch("   ")).toEqual({ note: null });
  });
});

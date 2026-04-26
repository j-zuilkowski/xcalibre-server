/**
 * Pure annotation helper functions for the EPUB reader.
 *
 * All functions are pure (no side effects, no API calls, no state mutations).
 * They operate on arrays of {@link BookAnnotation} and return new arrays or objects.
 * State updates in the reader are performed by passing the results of these
 * functions to React state setters.
 *
 * Sorting:
 * - Annotations are sorted by CFI range string (lexicographic) so that highlights
 *   appear in document order. When two annotations share the same CFI, they are
 *   further sorted by `created_at`.
 *
 * Optimistic annotations:
 * - `createOptimisticAnnotation` builds a temporary annotation with a `"temp-"` id
 *   prefix before the server responds. The real server-assigned ID replaces the
 *   temp one once the create request resolves.
 */
import type {
  AnnotationColor,
  AnnotationType,
  BookAnnotation,
  CreateBookAnnotationRequest,
  PatchBookAnnotationRequest,
} from "@xs/shared";

/** Ordered list of available annotation highlight colors shown in the color picker. */
export const ANNOTATION_COLORS: AnnotationColor[] = ["yellow", "green", "blue", "pink"];

/**
 * Returns a new sorted copy of the annotation array.
 * Primary sort: `cfi_range` lexicographic (document order).
 * Secondary sort: `created_at` ascending (earlier annotations first at same position).
 */
export function sortAnnotations(annotations: BookAnnotation[]): BookAnnotation[] {
  return [...annotations].sort((left, right) => {
    const cfiSort = left.cfi_range.localeCompare(right.cfi_range);
    if (cfiSort !== 0) {
      return cfiSort;
    }

    return left.created_at.localeCompare(right.created_at);
  });
}

/**
 * Inserts or replaces an annotation in the list (matched by `id`), then re-sorts.
 * Used for both optimistic inserts and for reconciling server responses.
 * Returns a new array; does not mutate the input.
 */
export function upsertAnnotation(annotations: BookAnnotation[], annotation: BookAnnotation): BookAnnotation[] {
  const next = annotations.some((entry) => entry.id === annotation.id)
    ? annotations.map((entry) => (entry.id === annotation.id ? annotation : entry))
    : [...annotations, annotation];

  return sortAnnotations(next);
}

/**
 * Returns a new array with the annotation identified by `annotationId` removed.
 * A no-op (returns original reference) when the id is not found.
 */
export function removeAnnotation(annotations: BookAnnotation[], annotationId: string): BookAnnotation[] {
  return annotations.filter((entry) => entry.id !== annotationId);
}

/**
 * Returns the best human-readable preview text for an annotation.
 * Priority: note (trimmed) → highlighted_text (trimmed) → cfi_range.
 * Used in the annotation list panel and the annotation edit sheet preview.
 */
export function annotationPreviewText(annotation: BookAnnotation): string {
  const note = annotation.note?.trim();
  if (note) {
    return note;
  }

  const highlightedText = annotation.highlighted_text?.trim();
  if (highlightedText) {
    return highlightedText;
  }

  return annotation.cfi_range;
}

/**
 * Maps an annotation type to its Ionicons icon name for rendering in the list panel.
 * - `"bookmark"` → `"bookmark-outline"`
 * - `"note"` → `"create-outline"`
 * - `"highlight"` → `"pencil-outline"`
 */
export function annotationIconName(type: AnnotationType): "bookmark-outline" | "create-outline" | "pencil-outline" {
  if (type === "bookmark") {
    return "bookmark-outline";
  }

  if (type === "note") {
    return "create-outline";
  }

  return "pencil-outline";
}

/**
 * Builds a {@link PatchBookAnnotationRequest} that updates only the color field.
 * Used when the user picks a new highlight color in the annotation edit sheet.
 */
export function annotationColorPatch(color: AnnotationColor): PatchBookAnnotationRequest {
  return { color };
}

/**
 * Builds a {@link PatchBookAnnotationRequest} that updates only the note field.
 * Trims whitespace and converts blank strings to null (clearing the note).
 */
export function annotationNotePatch(note: string): PatchBookAnnotationRequest {
  const trimmed = note.trim();
  return { note: trimmed.length > 0 ? trimmed : null };
}

/**
 * Returns a shallow copy of the annotation with the color field replaced.
 * Used for optimistic color updates before the PATCH response arrives.
 */
export function updateAnnotationColor(annotation: BookAnnotation, color: AnnotationColor): BookAnnotation {
  return { ...annotation, color };
}

/**
 * Returns a shallow copy of the annotation with the note field replaced.
 * Trims whitespace; sets `note` to null when the trimmed string is empty.
 * Used for optimistic note updates before the PATCH response arrives.
 */
export function updateAnnotationNote(annotation: BookAnnotation, note: string): BookAnnotation {
  const trimmed = note.trim();
  return { ...annotation, note: trimmed.length > 0 ? trimmed : null };
}

/**
 * Creates a temporary local annotation from a {@link CreateBookAnnotationRequest}
 * before the server `POST /api/v1/books/:id/annotations` call completes.
 *
 * The temporary `id` has the prefix `"temp-"` followed by a timestamp and
 * random suffix to ensure uniqueness across concurrent optimistic creates.
 * `user_id` is set to `"optimistic"` as a sentinel value.
 *
 * Once the server responds, the caller should:
 * 1. Remove the optimistic annotation (by temp id).
 * 2. Upsert the real server-assigned annotation.
 */
export function createOptimisticAnnotation(
  bookId: string,
  request: CreateBookAnnotationRequest,
): BookAnnotation {
  const now = new Date().toISOString();

  return {
    id: `temp-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    user_id: "optimistic",
    book_id: bookId,
    type: request.type,
    cfi_range: request.cfi_range,
    highlighted_text: request.highlighted_text ?? null,
    note: request.note ?? null,
    color: request.color ?? "yellow",
    created_at: now,
    updated_at: now,
  };
}

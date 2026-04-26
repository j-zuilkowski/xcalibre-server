/**
 * Shared TypeScript types for the xcalibre-server API.
 *
 * Used by both the web (React + Vite) and mobile (Expo) clients.
 * All types correspond directly to the JSON shapes returned or accepted
 * by the Rust/Axum backend at `/api/v1/*`.
 *
 * Key domain concepts:
 * - {@link Book} / {@link BookSummary} — the primary library entity
 * - {@link BookAnnotation} — reader annotations anchored by EPUB CFI range
 * - {@link ReadingProgress} — per-format reading cursor (CFI for EPUB, page for PDF)
 * - {@link Shelf} — user-curated ordered list of books
 * - {@link CollectionSummary} — AI-powered thematic collection with semantic chunks
 * - {@link UserStats} — reading streak, progress, and top-authors/tags
 */

/**
 * Role assigned to a {@link User}. Controls which write operations the user may
 * perform. All fields except `id` and `name` are optional and default to false
 * when absent, meaning read-only access.
 */
export type Role = {
  /** UUID of the role row. */
  id: string;
  /** Human-readable role label, e.g. "Admin" or "Reader". */
  name: string;
  /** Whether the user may upload new book files. */
  can_upload?: boolean;
  /** Whether the user may trigger bulk import jobs. */
  can_bulk?: boolean;
  /** Whether the user may edit book metadata. */
  can_edit?: boolean;
  /** Whether the user may download book files. */
  can_download?: boolean;
};

/**
 * Authenticated user account returned by `/api/v1/auth/me` and embedded in
 * {@link AuthSession}. The `role` field drives UI permission gating on both
 * web and mobile.
 */
export type User = {
  /** UUID of the user row. */
  id: string;
  /** Unique login handle; used as the "username" field in {@link LoginRequest}. */
  username: string;
  /** Email address; unique per server. */
  email: string;
  /** The role that controls which write operations the user may perform. */
  role: Role;
  /** Whether the account is active. Inactive accounts cannot authenticate. */
  is_active: boolean;
  /** When true the server will require the user to change their password on next login. */
  force_pw_reset: boolean;
  /** UUID of the library the user has selected as their default. */
  default_library_id: string;
  /** Whether TOTP two-factor authentication is configured for this account. */
  totp_enabled: boolean;
  /** ISO-8601 timestamp of account creation. */
  created_at: string;
  /** ISO-8601 timestamp of the last modification to the user record. */
  last_modified: string;
};

/** Lightweight author reference embedded in {@link Book}, {@link BookSummary}, and merge responses. */
export type AuthorRef = {
  /** UUID of the author row. */
  id: string;
  /** Display name as entered in the library, e.g. "Ursula K. Le Guin". */
  name: string;
  /** Normalized sort form, e.g. "Le Guin, Ursula K.". */
  sort_name: string;
};

/** Optional enrichment data for an author, sourced from OpenLibrary or manually set. */
export type AuthorProfile = {
  /** Free-form biographical text. */
  bio: string | null;
  /** URL of the author photo served by the backend. */
  photo_url: string | null;
  /** Year of birth as a string, e.g. "1929". */
  born: string | null;
  /** Year of death as a string; null for living authors. */
  died: string | null;
  /** Author's personal or publisher website URL. */
  website_url: string | null;
  /** OpenLibrary author key, e.g. "OL1234A". Used for metadata lookups. */
  openlibrary_id: string | null;
};

/**
 * Full author record returned by `GET /api/v1/authors/:id`.
 * Includes a paginated slice of the author's books.
 */
export type AuthorDetail = {
  /** UUID of the author row. */
  id: string;
  /** Display name. */
  name: string;
  /** Normalized sort form. */
  sort_name: string;
  /** Optional enrichment profile; null if no profile has been created. */
  profile: AuthorProfile | null;
  /** Total number of books attributed to this author across all libraries. */
  book_count: number;
  /** The current page of books for this author (paginated). */
  books: BookSummary[];
  /** Current page number (1-based). */
  page: number;
  /** Number of books per page. */
  page_size: number;
};

/** Author row as returned by the admin author list endpoint. */
export type AdminAuthor = {
  /** UUID of the author row. */
  id: string;
  /** Display name. */
  name: string;
  /** Normalized sort form. */
  sort_name: string;
  /** Total books attributed to this author. */
  book_count: number;
  /** Whether an {@link AuthorProfile} has been created for this author. */
  has_profile: boolean;
};

/** Partial update payload for `PATCH /api/v1/authors/:id`. All fields are optional. */
export type AuthorProfilePatch = {
  bio?: string | null;
  born?: string | null;
  died?: string | null;
  website_url?: string | null;
  openlibrary_id?: string | null;
};

/** Response from `POST /api/v1/admin/authors/:id/merge`. The source author is deleted. */
export type MergeAuthorResponse = {
  /** Number of book–author associations updated to point at the target author. */
  books_updated: number;
  /** The surviving (target) author after the merge. */
  target_author: AuthorRef;
};

/** Lightweight series reference embedded in {@link Book} and {@link BookSummary}. */
export type SeriesRef = {
  /** UUID of the series row. */
  id: string;
  /** Display name, e.g. "The Expanse". */
  name: string;
};

/**
 * Tag reference embedded in {@link Book}.
 * Tags may be sourced from calibre import, manual entry, or LLM classification.
 * The `confirmed` flag distinguishes LLM suggestions awaiting human review from
 * user-approved tags.
 */
export type TagRef = {
  /** UUID of the tag row. */
  id: string;
  /** Tag display name, e.g. "science fiction". */
  name: string;
  /**
   * Whether the tag has been confirmed by a human.
   * LLM-suggested tags start as unconfirmed (false) until a user or admin
   * approves them via the confirm endpoint.
   */
  confirmed: boolean;
};

/** Minimal tag shape returned by tag-search autocomplete. */
export type TagLookupItem = {
  id: string;
  name: string;
};

/**
 * Origin of a tag in the library.
 * - `"manual"` — created by a user through the UI or API
 * - `"llm"` — suggested by the LLM classifier; may be unconfirmed
 * - `"calibre_import"` — imported from a calibre database
 */
export type TagSource = "manual" | "llm" | "calibre_import";

/** Tag row returned by admin tag management endpoints. */
export type AdminTag = {
  /** UUID of the tag row. */
  id: string;
  /** Tag display name. */
  name: string;
  /** How the tag was created. */
  source: TagSource;
};

/** Admin tag enriched with usage counts for the tag management table. */
export type AdminTagWithCount = AdminTag & {
  /** Total books that carry this tag (confirmed or not). */
  book_count: number;
  /** Books where this tag has been confirmed by a human. */
  confirmed_count: number;
};

/** Response from `POST /api/v1/admin/tags/:id/merge`. The source tag is deleted. */
export type MergeTagResponse = {
  /** Number of book–tag associations moved to the target tag. */
  merged_book_count: number;
  /** The surviving (target) tag after the merge. */
  target_tag: AdminTag;
};

/**
 * Per-user tag content restriction.
 * - `"allow"` — only show books that have this tag (allowlist mode)
 * - `"block"` — hide books that have this tag (blocklist mode)
 */
export type UserTagRestriction = {
  user_id: string;
  tag_id: string;
  tag_name: string;
  mode: "allow" | "block";
};

/**
 * High-level document category assigned by the LLM classifier or manual edit.
 * Drives icon and layout choices in the reader and library UI.
 */
export type DocumentType =
  | "novel"
  | "textbook"
  | "reference"
  | "magazine"
  | "datasheet"
  | "comic"
  | "unknown";

/**
 * A single LLM-generated tag suggestion, part of a {@link ClassifyResult}.
 */
export type TagSuggestion = {
  /** Proposed tag display name. */
  name: string;
  /**
   * Model confidence score in the range 0.0–1.0.
   * Values closer to 1.0 indicate higher certainty.
   */
  confidence: number;
};

/**
 * Response from `GET /api/v1/books/:id/classify`.
 * Contains LLM-generated tag suggestions that a user can confirm or reject.
 */
export type ClassifyResult = {
  book_id: string;
  /** Ranked list of tag suggestions. May be empty if the model found nothing. */
  suggestions: TagSuggestion[];
  /** Identifier of the LLM model used for this classification run. */
  model_id: string;
  /** Number of tags in the `pending` (unconfirmed) state after this run. */
  pending_count: number;
};

/** A single metadata validation issue from the LLM validator. */
export type ValidationIssue = {
  /** Metadata field that has the problem, e.g. "title" or "description". */
  field: string;
  /** Whether this is an advisory warning or a blocking error. */
  severity: "warning" | "error";
  /** Human-readable description of the problem. */
  message: string;
  /** Optional corrective suggestion text from the model. */
  suggestion: string | null;
};

/** Response from `GET /api/v1/books/:id/validate`. */
export type ValidationResult = {
  book_id: string;
  /** Highest severity level found across all issues. `"ok"` means no issues. */
  severity: "ok" | "warning" | "error";
  issues: ValidationIssue[];
  /** Identifier of the LLM model used for this validation run. */
  model_id: string;
};

/** Response from `GET /api/v1/books/:id/derive`. LLM-generated book insights. */
export type DeriveResult = {
  book_id: string;
  /** Short LLM-generated summary of the book. */
  summary: string;
  /** Titles of related or similar books suggested by the model. */
  related_titles: string[];
  /** Book-club style discussion questions generated by the model. */
  discussion_questions: string[];
  /** Identifier of the LLM model used. */
  model_id: string;
};

/**
 * LLM feature availability from `GET /api/v1/llm/health`.
 * Used by the mobile Search screen to decide whether to show the "AI Semantic"
 * tab, and by the Book Detail screen to show or hide the AI panel.
 */
export type LlmHealth = {
  /** Whether `ENABLE_LLM_FEATURES` is true on the server. */
  enabled: boolean;
  librarian: {
    /** Whether the librarian (classify/validate/derive) model endpoint is reachable. */
    available: boolean;
    /** Model identifier string, e.g. "gpt-4o-mini"; null when unavailable. */
    model_id: string | null;
    /** Endpoint URL used by the server to reach the model. */
    endpoint: string;
  };
};

/**
 * A concrete file format associated with a book (e.g. EPUB, PDF, MOBI).
 * Each format has its own download URL constructed via `ApiClient.downloadUrl(bookId, format)`.
 */
export type FormatRef = {
  /** UUID of the format row. */
  id: string;
  /** Format label in upper-case, e.g. "EPUB", "PDF", "MOBI". */
  format: string;
  /** On-disk file size in bytes. Used for download progress and storage estimates. */
  size_bytes: number;
};

/** An external identifier for a book, such as ISBN-13 or ASIN. */
export type Identifier = {
  /** UUID of the identifier row. */
  id: string;
  /** Identifier scheme, e.g. "isbn", "amazon", "goodreads". */
  id_type: string;
  /** The raw identifier value. */
  value: string;
};

/** Value type for a user-defined custom column. Controls storage and rendering. */
export type CustomColumnType = "text" | "integer" | "float" | "bool" | "datetime";

/** Schema definition for a user-defined custom metadata column. */
export type CustomColumn = {
  /** UUID of the column definition. */
  id: string;
  /** Internal machine name, e.g. "read_count". */
  name: string;
  /** Human-readable label shown in the UI. */
  label: string;
  /** Value type; determines how the column is stored and rendered. */
  column_type: CustomColumnType;
  /** Whether this column can hold multiple values (comma-separated in text columns). */
  is_multiple: boolean;
};

/** The value a book has stored for a specific custom column. */
export type BookCustomValue = {
  /** UUID of the {@link CustomColumn} this value belongs to. */
  column_id: string;
  /** Human-readable label of the column (denormalized for convenience). */
  label: string;
  /** Value type from the column definition. */
  column_type: CustomColumnType;
  /** Actual stored value; null when the field has not been set for this book. */
  value: string | number | boolean | null;
};

/** An update to a single custom-column value sent to `PATCH /api/v1/books/:id/custom-values`. */
export type BookCustomValuePatch = {
  /** UUID of the {@link CustomColumn} to update. */
  column_id: string;
  /** New value; null clears the field. */
  value: string | number | boolean | null;
};

/**
 * Reading position cursor returned by `GET /api/v1/books/:id/progress`.
 *
 * The position is stored in two complementary forms:
 * - `cfi` — EPUB Canonical Fragment Identifier used by foliojs-port to jump
 *   to an exact location within the reflowable text.
 * - `page` — physical page number used by the PDF reader.
 * - `percentage` — normalized 0.0–1.0 position for both formats; used for
 *   the progress bar and offline cache.
 *
 * On mobile, progress is also mirrored to `local_sync_state` in Expo SQLite
 * so the book detail screen can display it without a network call.
 */
export type ReadingProgress = {
  /** UUID of the progress row. */
  id: string;
  /** Book this progress record belongs to. */
  book_id: string;
  /** The specific format file (UUID) the user was reading. */
  format_id: string;
  /**
   * EPUB CFI string for reflowable formats (EPUB/MOBI).
   * Null for PDF or when progress has not yet been recorded.
   */
  cfi: string | null;
  /**
   * 1-based page number for PDF formats.
   * Null for EPUB or when progress has not yet been recorded.
   */
  page: number | null;
  /**
   * Normalized reading position in the range 0.0–1.0.
   * Values sent from older clients may arrive as 0–100; the server normalises.
   */
  percentage: number;
  /** ISO-8601 timestamp of the last server-side update. */
  updated_at: string;
  /** ISO-8601 timestamp used by the delta-sync cursor on mobile. */
  last_modified: string;
};

/**
 * Payload for `PATCH /api/v1/books/:id/progress`.
 * Sent by both readers after every debounced position change (~2 s).
 * `percentage` is required; `cfi` and `page` are format-specific.
 */
export type ReadingProgressPatch = {
  /** Format label string, e.g. "EPUB". Mutually exclusive with `format_id`. */
  format?: string;
  /** UUID of the format row. Preferred over `format` when available. */
  format_id?: string;
  /** Updated CFI for EPUB/MOBI. Null to clear. */
  cfi?: string | null;
  /** Updated page number for PDF. Null to clear. */
  page?: number | null;
  /** Normalized position 0.0–1.0. Required. */
  percentage: number;
};

/**
 * Kind of reader annotation.
 * - `"highlight"` — coloured text selection, no note required
 * - `"note"` — text selection with an attached note
 * - `"bookmark"` — position marker with no selection; `cfi_range` is the
 *   current reading location at the time the bookmark was created
 */
export type AnnotationType = "highlight" | "note" | "bookmark";

/** Highlight color chosen by the reader for an annotation. */
export type AnnotationColor = "yellow" | "green" | "blue" | "pink";

/**
 * A reader annotation stored by the server.
 *
 * Annotations are anchored to a specific position in the EPUB using a CFI range
 * string produced by foliojs-port. The mobile EPUB reader syncs annotations on
 * mount and applies them as overlays in the renderer.
 *
 * Optimistic annotations (created locally before the server responds) use a
 * temporary id with the prefix `"temp-"` and `user_id: "optimistic"`.
 */
export type BookAnnotation = {
  /** UUID of the annotation row. Temporary ids start with "temp-". */
  id: string;
  /** UUID of the user who created the annotation. */
  user_id: string;
  /** UUID of the book this annotation belongs to. */
  book_id: string;
  /** Annotation kind. */
  type: AnnotationType;
  /**
   * EPUB Canonical Fragment Identifier range string identifying the annotated
   * passage, e.g. `"epubcfi(/6/4!/4/2/1:0,/1:20)"`.
   * For bookmarks this is the single-location CFI of the current reading position.
   */
  cfi_range: string;
  /** The raw text selected by the user; null for bookmarks. */
  highlighted_text: string | null;
  /** Free-text note attached to the annotation; null when none was added. */
  note: string | null;
  /** Highlight color chosen in the annotation sheet. */
  color: AnnotationColor;
  /** ISO-8601 creation timestamp. */
  created_at: string;
  /** ISO-8601 last-update timestamp. */
  updated_at: string;
};

/** Payload for `POST /api/v1/books/:id/annotations`. */
export type CreateBookAnnotationRequest = {
  type: AnnotationType;
  /** CFI range produced by the EPUB renderer on text selection. */
  cfi_range: string;
  /** Verbatim selected text; omit or null for bookmarks. */
  highlighted_text?: string | null;
  /** Optional note text; can be added or changed later via patch. */
  note?: string | null;
  /** Defaults to `"yellow"` when omitted. */
  color?: AnnotationColor;
};

/**
 * Partial update payload for `PATCH /api/v1/books/:id/annotations/:annotationId`.
 * Only the fields provided are updated; absent fields are left unchanged.
 */
export type PatchBookAnnotationRequest = {
  /** Updated note text; null clears the note. */
  note?: string | null;
  /** Updated highlight color. */
  color?: AnnotationColor;
};

/** {@link User} extended with admin-only fields returned by `/api/v1/admin/users`. */
export type AdminUser = User & {
  /** ISO-8601 timestamp of the user's most recent successful login; null if never logged in. */
  last_login_at: string | null;
};

/** Payload for `POST /api/v1/admin/users`. */
export type AdminUserCreateRequest = {
  username: string;
  email: string;
  /** Plain-text password; the server hashes it before storage. */
  password: string;
  /** UUID of the role to assign; defaults to the server's default reader role. */
  role_id?: string;
  /** Whether the account starts active; defaults to true. */
  is_active?: boolean;
};

/** Partial update payload for `PATCH /api/v1/admin/users/:id`. */
export type AdminUserUpdateRequest = {
  /** UUID of the new role to assign. */
  role_id?: string;
  /** Activate or deactivate the account. */
  is_active?: boolean;
  /** When true, forces the user to set a new password on next login. */
  force_pw_reset?: boolean;
};

/** Background job row returned by `/api/v1/admin/jobs`. */
export type AdminJob = {
  /** UUID of the job row. */
  id: string;
  /** Job type string, e.g. "classify", "validate", "derive", "import". */
  job_type: string;
  /** Current lifecycle state. Terminal states are `"completed"` and `"failed"`. */
  status: "pending" | "running" | "completed" | "failed";
  /** UUID of the book this job is operating on; null for non-book jobs. */
  book_id: string | null;
  /** Denormalized book title for display; null when `book_id` is null. */
  book_title: string | null;
  /** ISO-8601 creation timestamp. */
  created_at: string;
  /** ISO-8601 timestamp when the worker picked up the job; null if still queued. */
  started_at: string | null;
  /** ISO-8601 timestamp when the job finished (success or failure). */
  completed_at: string | null;
  /** Error message text when status is `"failed"`; null otherwise. */
  error_text: string | null;
};

/**
 * Scheduled task types available for recurring automation.
 * - `"classify_all"` — run LLM classification on all unclassified books
 * - `"semantic_index_all"` — rebuild the full vector search index
 * - `"backup"` — create a database backup archive
 */
export type ScheduledTaskType = "classify_all" | "semantic_index_all" | "backup";

/** A configured recurring task returned by `/api/v1/admin/scheduled-tasks`. */
export type ScheduledTask = {
  id: string;
  /** Display name given by the admin. */
  name: string;
  task_type: ScheduledTaskType;
  /** Standard cron expression (5-field), e.g. `"0 2 * * *"` (daily at 02:00). */
  cron_expr: string;
  /** Whether the scheduler will run this task when its cron fires. */
  enabled: boolean;
  /** ISO-8601 timestamp of the last execution attempt; null if never run. */
  last_run_at: string | null;
  /** ISO-8601 timestamp of the next scheduled execution; null when disabled. */
  next_run_at: string | null;
  created_at: string;
};

export type ScheduledTaskCreateRequest = {
  name: string;
  task_type: ScheduledTaskType;
  cron_expr: string;
  enabled: boolean;
};

export type ScheduledTaskPatchRequest = {
  enabled?: boolean;
  cron_expr?: string;
};

/**
 * Webhook event names the server can deliver to a registered webhook URL.
 * Each event sends a JSON payload to the subscriber's HTTPS endpoint.
 */
export type WebhookEventName =
  | "book.added"
  | "book.deleted"
  | "import.completed"
  | "llm_job.completed"
  | "user.registered";

/** A registered webhook subscription. Secrets are write-only and not returned after creation. */
export type Webhook = {
  id: string;
  /** HTTPS URL the server will POST event payloads to. */
  url: string;
  /** List of event names this webhook is subscribed to. */
  events: WebhookEventName[];
  enabled: boolean;
  /** ISO-8601 timestamp of the last successful delivery; null if never delivered. */
  last_delivery_at: string | null;
  /** Error message from the last failed delivery attempt; null on success. */
  last_error: string | null;
  created_at: string;
};

export type WebhookCreateRequest = {
  url: string;
  secret: string;
  events: WebhookEventName[];
};

export type WebhookUpdateRequest = {
  url?: string;
  events?: WebhookEventName[];
  enabled?: boolean;
};

export type WebhookTestResponse = {
  delivered: boolean;
  response_status: number | null;
  error: string | null;
};

export type UpdateCheckResponse = {
  current_version?: string;
  latest_version?: string;
  update_available: boolean;
  release_url?: string;
  error?: string;
};

export type KoboDevice = {
  id: string;
  user_id: string;
  username: string;
  email: string;
  device_id: string;
  device_name: string;
  last_sync_at: string | null;
  created_at: string;
};

/**
 * A calibre library registered with the server.
 * Multiple libraries can be registered; users select a default via
 * `PATCH /api/v1/users/me/library`.
 */
export type Library = {
  /** UUID of the library row. */
  id: string;
  /** Human-readable display name. */
  name: string;
  /** Absolute path to the calibre `metadata.db` file on the server. */
  calibre_db_path: string;
  created_at: string;
  updated_at: string;
  /** Total books indexed from this library; included only in list responses. */
  book_count?: number;
};

/** A single chapter extracted from a book's table of contents. */
export type Chapter = {
  /** Zero-based chapter index within the book. */
  index: number;
  /** Chapter title as found in the TOC. */
  title: string;
  /** Approximate word count for this chapter (used for progress estimation). */
  word_count: number;
};

/** Chapter list returned by `GET /api/v1/books/:id/chapters`. */
export type BookChapters = {
  book_id: string;
  /** Format the chapter list was extracted from, e.g. "EPUB". */
  format: string;
  chapters: Chapter[];
};

/** Plain-text content returned by `GET /api/v1/books/:id/text`. Used by the MCP tools. */
export type BookText = {
  book_id: string;
  /** Format the text was extracted from, e.g. "EPUB". */
  format: string;
  /** Zero-based chapter index if a specific chapter was requested; null for full text. */
  chapter: number | null;
  /** Raw extracted text. */
  text: string;
  /** Word count of the returned text. */
  word_count: number;
};

/** A name–count pair used in ranked lists within {@link UserStats}. */
export type StatCountItem = {
  name: string;
  count: number;
};

/** Books completed per calendar month, for the monthly reading chart. */
export type MonthlyBookCount = {
  /** ISO year-month string, e.g. "2026-04". */
  month: string;
  /** Number of books marked read in that month. */
  count: number;
};

/**
 * Aggregated reading statistics returned by `GET /api/v1/users/me/stats`.
 * Displayed in the mobile Stats screen and summarized in the Profile tab.
 */
export type UserStats = {
  /** Total books the user has marked as read (all time). */
  total_books_read: number;
  /** Books marked read in the current calendar year. */
  books_read_this_year: number;
  /** Books marked read in the current calendar month. */
  books_read_this_month: number;
  /** Books currently in progress (progress > 0 and not marked read). */
  books_in_progress: number;
  /** Total number of progress-update events recorded across all sessions. */
  total_reading_sessions: number;
  /** Current consecutive-days reading streak. */
  reading_streak_days: number;
  /** All-time longest consecutive-days streak. */
  longest_streak_days: number;
  /**
   * Mean progress percentage advanced per reading session.
   * Expressed as a percentage point value (e.g. 3.5 means 3.5 pp per session).
   */
  average_progress_per_session: number;
  /** Map of format label → number of books read in that format. */
  formats_read: Record<string, number>;
  /** Top 3 (or more) tags by book count. */
  top_tags: StatCountItem[];
  /** Top 3 (or more) authors by book count. */
  top_authors: StatCountItem[];
  /** Per-month reading counts for the chart (last 12 months). */
  monthly_books: MonthlyBookCount[];
};

/** Server-wide statistics returned by `GET /api/v1/admin/system`. */
export type SystemStats = {
  /** Xcalibre-server server version string. */
  version: string;
  /** Database engine the server is running against. */
  db_engine: "sqlite" | "mariadb";
  /** Size of the primary database file in bytes. */
  db_size_bytes: number;
  /** Total books across all registered libraries. */
  book_count: number;
  /** Total distinct format files stored. */
  format_count: number;
  /** Total bytes used by all stored book files. */
  storage_used_bytes: number;
  meilisearch: {
    /** Whether Meilisearch is reachable from the server. */
    available: boolean;
    /** Number of books currently indexed in Meilisearch. */
    indexed_count: number;
    /** Books awaiting indexing in the background queue. */
    pending_count: number;
  };
  llm: {
    enabled: boolean;
    librarian_available: boolean;
    architect_available: boolean;
  };
};

export type ImportStatus = {
  id: string;
  status: "pending" | "running" | "completed" | "failed";
  dry_run: boolean;
  records_total: number;
  records_imported: number;
  records_failed: number;
  records_skipped: number;
  failures: Array<{ file: string; reason: string }>;
  started_at: string;
  completed_at: string | null;
};

export type BulkImportRequest = {
  source: "upload" | "path";
  path?: string;
  file?: File;
  dry_run?: boolean;
};

export type BulkImportResponse = {
  job_id: string;
};

export type ReadingImportSource = "goodreads" | "storygraph";

export type ReadingImportError = {
  row: number;
  title: string;
  author: string;
  reason: string;
};

export type ReadingImportJob = {
  id: string;
  filename: string;
  source: ReadingImportSource;
  status: "pending" | "running" | "complete" | "failed";
  total_rows: number | null;
  matched: number;
  unmatched: number;
  errors: ReadingImportError[];
  created_at: string;
  completed_at: string | null;
};

export type ReadingImportResponse = {
  job_id: string;
};

/**
 * A user-curated shelf returned by `GET /api/v1/shelves`.
 * Shelves are ordered lists of books. Mobile supports a "Download all"
 * action on the shelf detail screen.
 */
export type Shelf = {
  /** UUID of the shelf row. */
  id: string;
  /** Display name chosen by the user. */
  name: string;
  /** Whether other users can browse this shelf. */
  is_public: boolean;
  /** Total books currently on this shelf. */
  book_count: number;
  /** ISO-8601 creation timestamp. */
  created_at: string;
  /**
   * ISO-8601 last-modification timestamp.
   * Used as the delta-sync cursor on mobile: the app fetches shelves modified
   * after the locally stored `last_modified` value.
   */
  last_modified: string;
};

/**
 * Subject domain of a {@link CollectionSummary}.
 * Drives the semantic chunking strategy and influences vector search ranking.
 */
export type CollectionDomain =
  | "technical"
  | "electronics"
  | "culinary"
  | "legal"
  | "academic"
  | "narrative";

/**
 * An AI-powered thematic collection that groups books for semantic search.
 * Books in a collection are chunked and embedded into the sqlite-vec vector store
 * to support the semantic search feature.
 */
export type CollectionSummary = {
  id: string;
  name: string;
  description: string | null;
  domain: CollectionDomain;
  is_public: boolean;
  /** Number of books currently added to this collection. */
  book_count: number;
  /** Number of text chunks currently indexed in the vector store for this collection. */
  total_chunks: number;
  created_at: string;
  updated_at: string;
};

export type CollectionDetail = CollectionSummary & {
  books: BookSummary[];
};

export type CollectionCreateRequest = {
  name: string;
  description?: string | null;
  domain?: CollectionDomain;
  is_public?: boolean;
};

export type CollectionUpdateRequest = {
  name?: string;
  description?: string | null;
  domain?: CollectionDomain;
  is_public?: boolean;
};

export type CollectionBooksRequest = {
  book_ids: string[];
};

/**
 * Full book record returned by `GET /api/v1/books/:id`.
 * For list views use the lighter {@link BookSummary}.
 */
export type Book = {
  /** UUID of the book row. */
  id: string;
  /** Display title. */
  title: string;
  /** Normalized sort form, e.g. "Hitchhiker's Guide to the Galaxy, The". */
  sort_title: string;
  /** Publisher's book description / blurb; null when absent. */
  description: string | null;
  /** ISO-8601 publication date or year string; null when unknown. */
  pubdate: string | null;
  /** BCP-47 language code, e.g. "en", "fr"; null when not set. */
  language: string | null;
  /**
   * Book rating on a 0–10 integer scale (calibre convention).
   * Divide by 2 to convert to the more familiar 0–5 star scale.
   * Null when no rating has been set.
   */
  rating: number | null;
  document_type: DocumentType;
  /** Series this book belongs to; null for standalone books. */
  series: SeriesRef | null;
  /** Position within the series, e.g. 1.0 for the first book. Null when not in a series. */
  series_index: number | null;
  /** Authors listed on this book. */
  authors: AuthorRef[];
  /** Tags attached to this book (confirmed and unconfirmed). */
  tags: TagRef[];
  /** Available download formats for this book. */
  formats: FormatRef[];
  /** Absolute URL to the cover image served by the backend; null when no cover. */
  cover_url: string | null;
  /** Whether a cover image exists; prefer this over a null check on `cover_url`. */
  has_cover: boolean;
  /** Whether the current user has marked this book as read. */
  is_read: boolean;
  /** Whether the book has been archived (hidden from default library views). */
  is_archived: boolean;
  /** External identifiers such as ISBN and Goodreads IDs. */
  identifiers: Identifier[];
  /** ISO-8601 creation timestamp. */
  created_at: string;
  /**
   * ISO-8601 last-modification timestamp.
   * Used as the delta-sync cursor: mobile fetches books with
   * `last_modified > lastSyncTime` to pull only changed records.
   */
  last_modified: string;
  /** ISO-8601 timestamp of the last Meilisearch index run; null if not yet indexed. */
  indexed_at: string | null;
};

/**
 * Lightweight book record returned by the book-list and search endpoints.
 * Used to populate the library grid, search results, and shelf views.
 *
 * The mobile sync stores a subset of this type in the `local_books` SQLite
 * table so that the library grid can render offline.
 */
export type BookSummary = Pick<
  Book,
  | "id"
  | "title"
  | "sort_title"
  | "authors"
  | "series"
  | "series_index"
  | "cover_url"
  | "has_cover"
  | "is_read"
  | "is_archived"
  | "language"
  | "rating"
  | "document_type"
  | "last_modified"
> & {
  /**
   * Optional reading progress for the current user, 0.0–1.0.
   * Included when the server has a {@link ReadingProgress} record for this
   * book and user. Used to render the progress bar on book cards.
   */
  progress_percentage?: number;
};

/**
 * Standard paginated envelope returned by all list endpoints.
 * @template T Type of each item in the `items` array.
 */
export type PaginatedResponse<T> = {
  /** The items for the requested page. */
  items: T[];
  /** Total matching records across all pages. */
  total: number;
  /** The current page number (1-based). */
  page: number;
  /** Maximum items per page as requested. */
  page_size: number;
};

/**
 * A single search result extending {@link BookSummary} with an optional
 * relevance score.
 */
export type SearchResultItem = BookSummary & {
  /**
   * Relevance score in the range 0.0–1.0, present only for semantic search
   * results. Higher values indicate better semantic similarity to the query.
   */
  score?: number;
};

export type SearchSuggestionsResponse = {
  suggestions: string[];
};

export type SearchStatusResponse = {
  fts: boolean;
  meilisearch: boolean;
  semantic: boolean;
  backend: string;
};

export type MetadataLookupResponse = {
  source: "openlibrary" | "googlebooks";
  title: string;
  authors: string[];
  description: string | null;
  publisher: string | null;
  published_date: string | null;
  cover_url: string | null;
  isbn_13: string | null;
  categories: string[];
};

/**
 * Shape of the error thrown by {@link ApiClient} when a request returns a
 * non-2xx HTTP status. Callers can narrow on `status` for specific handling,
 * e.g. `status === 401` for auth errors or `status === 404` for not-found.
 */
export type ApiError = {
  /** Human-readable error message. */
  message: string;
  /** HTTP status code, e.g. 400, 401, 404, 422, 500. */
  status: number;
  /** Optional structured error details from the server response body. */
  details?: unknown;
};

/** Credentials sent to `POST /api/v1/auth/login`. */
export type LoginRequest = {
  /** The user's `username` field (not email). */
  username: string;
  password: string;
};

/**
 * A fully authenticated session.
 * On mobile, `access_token` is stored in Expo SecureStore under the key
 * `"access_token"` (Keychain on iOS, Keystore on Android) and retrieved by
 * the `ApiClient` constructor's `getToken` callback.
 * `refresh_token` is stored under `"refresh_token"` and used for silent
 * token renewal.
 */
export type AuthSession = {
  /** Short-lived JWT used as the `Authorization: Bearer` token on all requests. */
  access_token: string;
  /** Long-lived opaque token used to obtain a new `access_token` via the refresh endpoint. */
  refresh_token: string;
  /** The authenticated user record embedded in the session. */
  user: User;
};

/**
 * Returned by `POST /api/v1/auth/login` when the account has TOTP enabled.
 * The client must call `POST /api/v1/auth/totp/verify` with this `totp_token`
 * and the 6-digit code (or a backup code via `/totp/verify-backup`) to
 * complete authentication and receive an {@link AuthSession}.
 */
export type LoginTotpRequiredResponse = {
  totp_required: true;
  /** Temporary bearer token valid only for the TOTP verification step. */
  totp_token: string;
};

/**
 * Union response from `POST /api/v1/auth/login`.
 * Discriminate on `"totp_required" in response` to detect the TOTP challenge path.
 */
export type LoginResponse = AuthSession | LoginTotpRequiredResponse;

/**
 * Response from `POST /api/v1/auth/refresh`.
 * Both tokens are rotated on each refresh; the old refresh token is invalidated.
 */
export type RefreshResponse = {
  /** New short-lived access JWT. */
  access_token: string;
  /** New long-lived refresh token (rotation). */
  refresh_token: string;
};

export type AuthProvidersResponse = {
  google: boolean;
  github: boolean;
};

export type RegisterRequest = {
  username: string;
  email: string;
  password: string;
};

/**
 * Query parameters for `GET /api/v1/books`.
 * All fields are optional; omitting them returns all books in default sort order.
 */
export type ListBooksParams = {
  /** Full-text search query string. */
  q?: string;
  /** Filter by author UUID. */
  author_id?: string;
  /** Filter by series UUID. */
  series_id?: string;
  /** Filter by tag name(s). Multiple values are ANDed. */
  tag?: string | string[];
  /** Filter by BCP-47 language code, e.g. "en". */
  language?: string;
  /** Filter by format label, e.g. "EPUB". */
  format?: string;
  /** Filter to books in a specific collection. */
  collection_id?: string;
  /** Sort field name, e.g. "title", "author", "rating", "created_at". */
  sort?: string;
  /** Sort direction: "asc" or "desc". */
  order?: string;
  /** Page number (1-based). */
  page?: number;
  /** Results per page. */
  page_size?: number;
  /**
   * Delta-sync cursor: ISO-8601 timestamp.
   * Returns only books modified after this time.
   * Used by the mobile sync to pull incremental updates.
   */
  since?: string;
  /** When true, include archived books. Defaults to false. */
  show_archived?: boolean;
  /** When true, return only books the user has marked as read. */
  only_read?: boolean;
};

/**
 * Query parameters for the search endpoints.
 * Extends {@link ListBooksParams} with a `semantic` flag that routes the
 * request to `GET /api/v1/search/semantic` (vector similarity) instead of
 * the default `GET /api/v1/search` (full-text via Meilisearch/SQLite FTS5).
 */
export type SearchQuery = ListBooksParams & {
  /**
   * When true, use vector semantic search instead of keyword full-text search.
   * Requires `ENABLE_LLM_FEATURES=true` and an indexed collection on the server.
   * The mobile search tab gates this behind a {@link LlmHealth} check.
   */
  semantic?: boolean;
};

/** A single entry in the server-side download history log. */
export type DownloadHistoryItem = {
  book_id: string;
  title: string;
  /** Format label in upper-case, e.g. "EPUB". */
  format: string;
  /** ISO-8601 timestamp when the download was initiated. */
  downloaded_at: string;
};

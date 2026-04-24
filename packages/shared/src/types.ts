export type Role = {
  id: string;
  name: string;
  can_upload?: boolean;
  can_bulk?: boolean;
  can_edit?: boolean;
  can_download?: boolean;
};

export type User = {
  id: string;
  username: string;
  email: string;
  role: Role;
  is_active: boolean;
  force_pw_reset: boolean;
  default_library_id: string;
  totp_enabled: boolean;
  created_at: string;
  last_modified: string;
};

export type AuthorRef = {
  id: string;
  name: string;
  sort_name: string;
};

export type SeriesRef = {
  id: string;
  name: string;
};

export type TagRef = {
  id: string;
  name: string;
  confirmed: boolean;
};

export type TagLookupItem = {
  id: string;
  name: string;
};

export type TagSource = "manual" | "llm" | "calibre_import";

export type AdminTag = {
  id: string;
  name: string;
  source: TagSource;
};

export type AdminTagWithCount = AdminTag & {
  book_count: number;
  confirmed_count: number;
};

export type MergeTagResponse = {
  merged_book_count: number;
  target_tag: AdminTag;
};

export type UserTagRestriction = {
  user_id: string;
  tag_id: string;
  tag_name: string;
  mode: "allow" | "block";
};

export type DocumentType =
  | "novel"
  | "textbook"
  | "reference"
  | "magazine"
  | "datasheet"
  | "comic"
  | "unknown";

export type TagSuggestion = {
  name: string;
  confidence: number;
};

export type ClassifyResult = {
  book_id: string;
  suggestions: TagSuggestion[];
  model_id: string;
  pending_count: number;
};

export type ValidationIssue = {
  field: string;
  severity: "warning" | "error";
  message: string;
  suggestion: string | null;
};

export type ValidationResult = {
  book_id: string;
  severity: "ok" | "warning" | "error";
  issues: ValidationIssue[];
  model_id: string;
};

export type DeriveResult = {
  book_id: string;
  summary: string;
  related_titles: string[];
  discussion_questions: string[];
  model_id: string;
};

export type LlmHealth = {
  enabled: boolean;
  librarian: {
    available: boolean;
    model_id: string | null;
    endpoint: string;
  };
};

export type FormatRef = {
  id: string;
  format: string;
  size_bytes: number;
};

export type Identifier = {
  id: string;
  id_type: string;
  value: string;
};

export type CustomColumnType = "text" | "integer" | "float" | "bool" | "datetime";

export type CustomColumn = {
  id: string;
  name: string;
  label: string;
  column_type: CustomColumnType;
  is_multiple: boolean;
};

export type BookCustomValue = {
  column_id: string;
  label: string;
  column_type: CustomColumnType;
  value: string | number | boolean | null;
};

export type BookCustomValuePatch = {
  column_id: string;
  value: string | number | boolean | null;
};

export type ReadingProgress = {
  id: string;
  book_id: string;
  format_id: string;
  cfi: string | null;
  page: number | null;
  percentage: number;
  updated_at: string;
  last_modified: string;
};

export type ReadingProgressPatch = {
  format?: string;
  format_id?: string;
  cfi?: string | null;
  page?: number | null;
  percentage: number;
};

export type AnnotationType = "highlight" | "note" | "bookmark";

export type AnnotationColor = "yellow" | "green" | "blue" | "pink";

export type BookAnnotation = {
  id: string;
  user_id: string;
  book_id: string;
  type: AnnotationType;
  cfi_range: string;
  highlighted_text: string | null;
  note: string | null;
  color: AnnotationColor;
  created_at: string;
  updated_at: string;
};

export type CreateBookAnnotationRequest = {
  type: AnnotationType;
  cfi_range: string;
  highlighted_text?: string | null;
  note?: string | null;
  color?: AnnotationColor;
};

export type PatchBookAnnotationRequest = {
  note?: string | null;
  color?: AnnotationColor;
};

export type AdminUser = User & {
  last_login_at: string | null;
};

export type AdminUserCreateRequest = {
  username: string;
  email: string;
  password: string;
  role_id?: string;
  is_active?: boolean;
};

export type AdminUserUpdateRequest = {
  role_id?: string;
  is_active?: boolean;
  force_pw_reset?: boolean;
};

export type AdminJob = {
  id: string;
  job_type: string;
  status: "pending" | "running" | "completed" | "failed";
  book_id: string | null;
  book_title: string | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  error_text: string | null;
};

export type ScheduledTaskType = "classify_all" | "semantic_index_all" | "backup";

export type ScheduledTask = {
  id: string;
  name: string;
  task_type: ScheduledTaskType;
  cron_expr: string;
  enabled: boolean;
  last_run_at: string | null;
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

export type Library = {
  id: string;
  name: string;
  calibre_db_path: string;
  created_at: string;
  updated_at: string;
  book_count?: number;
};

export type Chapter = {
  index: number;
  title: string;
  word_count: number;
};

export type BookChapters = {
  book_id: string;
  format: string;
  chapters: Chapter[];
};

export type BookText = {
  book_id: string;
  format: string;
  chapter: number | null;
  text: string;
  word_count: number;
};

export type StatCountItem = {
  name: string;
  count: number;
};

export type MonthlyBookCount = {
  month: string;
  count: number;
};

export type UserStats = {
  total_books_read: number;
  books_read_this_year: number;
  books_read_this_month: number;
  books_in_progress: number;
  total_reading_sessions: number;
  reading_streak_days: number;
  longest_streak_days: number;
  average_progress_per_session: number;
  formats_read: Record<string, number>;
  top_tags: StatCountItem[];
  top_authors: StatCountItem[];
  monthly_books: MonthlyBookCount[];
};

export type SystemStats = {
  version: string;
  db_engine: "sqlite" | "mariadb";
  db_size_bytes: number;
  book_count: number;
  format_count: number;
  storage_used_bytes: number;
  meilisearch: {
    available: boolean;
    indexed_count: number;
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

export type Shelf = {
  id: string;
  name: string;
  is_public: boolean;
  book_count: number;
  created_at: string;
  last_modified: string;
};

export type Book = {
  id: string;
  title: string;
  sort_title: string;
  description: string | null;
  pubdate: string | null;
  language: string | null;
  rating: number | null;
  document_type: DocumentType;
  series: SeriesRef | null;
  series_index: number | null;
  authors: AuthorRef[];
  tags: TagRef[];
  formats: FormatRef[];
  cover_url: string | null;
  has_cover: boolean;
  is_read: boolean;
  is_archived: boolean;
  identifiers: Identifier[];
  created_at: string;
  last_modified: string;
  indexed_at: string | null;
};

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
  progress_percentage?: number;
};

export type PaginatedResponse<T> = {
  items: T[];
  total: number;
  page: number;
  page_size: number;
};

export type SearchResultItem = BookSummary & {
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

export type ApiError = {
  message: string;
  status: number;
  details?: unknown;
};

export type LoginRequest = {
  username: string;
  password: string;
};

export type AuthSession = {
  access_token: string;
  refresh_token: string;
  user: User;
};

export type LoginTotpRequiredResponse = {
  totp_required: true;
  totp_token: string;
};

export type LoginResponse = AuthSession | LoginTotpRequiredResponse;

export type RefreshResponse = {
  access_token: string;
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

export type ListBooksParams = {
  q?: string;
  author_id?: string;
  series_id?: string;
  tag?: string | string[];
  language?: string;
  format?: string;
  sort?: string;
  order?: string;
  page?: number;
  page_size?: number;
  since?: string;
  show_archived?: boolean;
  only_read?: boolean;
};

export type SearchQuery = ListBooksParams & {
  semantic?: boolean;
};

export type DownloadHistoryItem = {
  book_id: string;
  title: string;
  format: string;
  downloaded_at: string;
};

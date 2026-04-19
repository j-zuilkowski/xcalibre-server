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
  status: string;
  book_id: string | null;
  book_title: string | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  error_text: string | null;
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
  series: SeriesRef | null;
  series_index: number | null;
  authors: AuthorRef[];
  tags: TagRef[];
  formats: FormatRef[];
  cover_url: string | null;
  has_cover: boolean;
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
  | "language"
  | "rating"
  | "last_modified"
>;

export type PaginatedResponse<T> = {
  items: T[];
  total: number;
  page: number;
  page_size: number;
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

export type LoginResponse = {
  access_token: string;
  refresh_token: string;
  user: User;
};

export type RefreshResponse = {
  access_token: string;
  refresh_token: string;
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
};

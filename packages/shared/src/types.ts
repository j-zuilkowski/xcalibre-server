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

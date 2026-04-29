import type {
  AdminJob,
  AdminUser,
  AuthProvidersResponse,
  AuthSession,
  Book,
  BookAnnotation,
  BookSummary,
  CollectionSummary,
  ImportStatus,
  Library,
  ApiToken,
  ReadingProgress,
  Role,
  SearchResultItem,
  Shelf,
  User,
} from "@xs/shared";

export const roleUser: Role = {
  id: "role-user",
  name: "user",
  can_upload: false,
  can_bulk: false,
  can_edit: false,
  can_download: true,
};

export const roleAdmin: Role = {
  id: "role-admin",
  name: "admin",
  can_upload: true,
  can_bulk: true,
  can_edit: true,
  can_download: true,
};

export function makeUser(overrides: Partial<User> = {}): User {
  const user: User = {
    id: "1",
    username: "u",
    email: "u@x.io",
    role: roleUser,
    is_active: true,
    force_pw_reset: false,
    default_library_id: "default",
    totp_enabled: false,
    created_at: "2026-04-19T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
  };

  return {
    ...user,
    ...overrides,
    role: overrides.role ?? roleUser,
  };
}

export function makeAdminUser(overrides: Partial<AdminUser> = {}): AdminUser {
  return {
    ...makeUser({ role: roleAdmin, ...overrides }),
    last_login_at: "2026-04-19T00:00:00Z",
  };
}

export function makeAuthSession(overrides: Partial<AuthSession> = {}): AuthSession {
  return {
    access_token: "tok",
    refresh_token: "rtok",
    user: makeUser(),
    ...overrides,
  };
}

export function makeBookSummary(overrides: Partial<BookSummary> = {}): BookSummary {
  return {
    id: "1",
    title: "Dune",
    sort_title: "Dune",
    authors: [
      {
        id: "a1",
        name: "Frank Herbert",
        sort_name: "Herbert, Frank",
      },
    ],
    series: null,
    series_index: null,
    cover_url: "/covers/1.jpg",
    has_cover: true,
    is_read: false,
    is_archived: false,
    language: "en",
    rating: 4,
    document_type: "novel",
    last_modified: "2026-04-19T00:00:00Z",
    progress_percentage: 50,
    ...overrides,
  };
}

export function makeBook(overrides: Partial<Book> = {}): Book {
  return {
    id: "1",
    title: "Dune",
    sort_title: "Dune",
    description: "A desert planet adventure.",
    pubdate: "1965-08-01",
    language: "en",
    rating: 8,
    document_type: "novel",
    series: {
      id: "s1",
      name: "Dune",
    },
    series_index: 1,
    authors: [
      {
        id: "a1",
        name: "Frank Herbert",
        sort_name: "Herbert, Frank",
      },
    ],
    tags: [
      {
        id: "t1",
        name: "sci-fi",
        confirmed: true,
      },
    ],
    formats: [
      {
        id: "f1",
        format: "epub",
        size_bytes: 1234,
      },
      {
        id: "f2",
        format: "pdf",
        size_bytes: 2345,
      },
    ],
    cover_url: "/covers/1.jpg",
    has_cover: true,
    is_read: false,
    is_archived: false,
    identifiers: [
      {
        id: "i1",
        id_type: "isbn13",
        value: "9780441172719",
      },
    ],
    created_at: "2026-04-19T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
    indexed_at: "2026-04-19T00:00:00Z",
    ...overrides,
  };
}

export function makeSearchResult(overrides: Partial<SearchResultItem> = {}): SearchResultItem {
  return {
    ...makeBookSummary(),
    score: 0.92,
    ...overrides,
  };
}

export function makeShelf(overrides: Partial<Shelf> = {}): Shelf {
  return {
    id: "shelf-1",
    name: "Favorites",
    is_public: false,
    book_count: 1,
    created_at: "2026-04-19T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
    ...overrides,
  };
}

export function makeJob(overrides: Partial<AdminJob> = {}): AdminJob {
  return {
    id: "job-1",
    job_type: "import",
    status: "pending",
    book_id: null,
    book_title: "Dune",
    created_at: "2026-04-19T00:00:00Z",
    started_at: null,
    completed_at: null,
    error_text: null,
    ...overrides,
  };
}

export function makeCollection(overrides: Partial<CollectionSummary> = {}): CollectionSummary {
  return {
    id: "c1",
    name: "Classics",
    description: null,
    domain: "narrative",
    is_public: false,
    book_count: 0,
    total_chunks: 0,
    created_at: "2026-04-19T00:00:00Z",
    updated_at: "2026-04-19T00:00:00Z",
    ...overrides,
  };
}

export function makeProgress(overrides: Partial<ReadingProgress> = {}): ReadingProgress {
  return {
    id: "p1",
    book_id: "1",
    format_id: "f1",
    cfi: "epubcfi(/6/2)",
    page: 12,
    percentage: 50,
    updated_at: "2026-04-19T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
    ...overrides,
  };
}

export function makeAnnotation(overrides: Partial<BookAnnotation> = {}): BookAnnotation {
  return {
    id: "ann-1",
    user_id: "1",
    book_id: "1",
    type: "highlight",
    cfi_range: "epubcfi(/6/2)",
    highlighted_text: "Arrakis",
    note: null,
    color: "yellow",
    created_at: "2026-04-19T00:00:00Z",
    updated_at: "2026-04-19T00:00:00Z",
    ...overrides,
  };
}

export function makeAuthProviders(overrides: Partial<AuthProvidersResponse> = {}): AuthProvidersResponse {
  return {
    google: false,
    github: false,
    ...overrides,
  };
}

export function makeLibrary(overrides: Partial<Library> = {}): Library {
  return {
    id: "lib-1",
    name: "Main",
    calibre_db_path: "/books/metadata.db",
    created_at: "2026-04-19T00:00:00Z",
    updated_at: "2026-04-19T00:00:00Z",
    book_count: 1,
    ...overrides,
  };
}

export function makeImportStatus(overrides: Partial<ImportStatus> = {}): ImportStatus {
  return {
    id: "job-1",
    status: "completed",
    dry_run: false,
    records_total: 1,
    records_imported: 1,
    records_failed: 0,
    records_skipped: 0,
    failures: [],
    started_at: "2026-04-19T00:00:00Z",
    completed_at: "2026-04-19T00:01:00Z",
    ...overrides,
  };
}

export function makeApiToken(overrides: Partial<ApiToken> = {}): ApiToken {
  return {
    id: "token-1",
    name: "Reader",
    created_by: "user-1",
    created_at: "2026-04-19T00:00:00Z",
    last_used_at: null,
    expires_at: null,
    scope: "write",
    ...overrides,
  };
}

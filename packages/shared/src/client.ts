import type {
  ApiError,
  AdminJob,
  AdminUser,
  AdminUserCreateRequest,
  AdminUserUpdateRequest,
  AuthProvidersResponse,
  BulkImportRequest,
  BulkImportResponse,
  Book,
  BookChapters,
  MetadataLookupResponse,
  BookSummary,
  BookText,
  ClassifyResult,
  CustomColumn,
  CustomColumnType,
  DeriveResult,
  BookCustomValue,
  BookCustomValuePatch,
  DownloadHistoryItem,
  ListBooksParams,
  LlmHealth,
  LoginRequest,
  LoginResponse,
  PaginatedResponse,
  ImportStatus,
  KoboDevice,
  Library,
  SearchQuery,
  SearchResultItem,
  SearchStatusResponse,
  SearchSuggestionsResponse,
  Shelf,
  TagLookupItem,
  ReadingProgress,
  ReadingProgressPatch,
  SystemStats,
  Role,
  RefreshResponse,
  RegisterRequest,
  User,
  UserTagRestriction,
  ValidationResult,
} from "./types";

type ClientOptions = {
  getRefreshToken?: () => string | null;
  onRefreshTokens?: (tokens: RefreshResponse) => void;
};

export class ApiClient {
  private refreshTokenCache: string | null = null;

  constructor(
    private readonly baseUrl: string,
    private readonly getToken: () => string | null,
    private readonly onUnauthorized: () => void,
    private readonly options: ClientOptions = {},
  ) {}

  async login(req: LoginRequest): Promise<LoginResponse> {
    const response = await this.requestJson<LoginResponse>(
      "/api/v1/auth/login",
      {
        method: "POST",
        body: JSON.stringify(req),
      },
      { retryOnUnauthorized: false, notifyUnauthorized: false },
    );
    this.rememberRefreshToken(response.refresh_token);
    return response;
  }

  async register(req: RegisterRequest): Promise<User> {
    return this.requestJson<User>(
      "/api/v1/auth/register",
      {
        method: "POST",
        body: JSON.stringify(req),
      },
      { retryOnUnauthorized: false, notifyUnauthorized: false },
    );
  }

  async refresh(refreshToken: string): Promise<RefreshResponse> {
    const response = await this.requestJson<RefreshResponse>(
      "/api/v1/auth/refresh",
      {
        method: "POST",
        body: JSON.stringify({ refresh_token: refreshToken }),
      },
      { retryOnUnauthorized: false, notifyUnauthorized: false },
    );
    this.rememberRefreshToken(response.refresh_token);
    this.options.onRefreshTokens?.(response);
    return response;
  }

  async logout(refreshToken: string): Promise<void> {
    await this.requestJson<void>("/api/v1/auth/logout", {
      method: "POST",
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    this.refreshTokenCache = null;
  }

  async me(): Promise<User> {
    return this.requestJson<User>("/api/v1/auth/me");
  }

  async getAuthProviders(): Promise<AuthProvidersResponse> {
    return this.requestJson<AuthProvidersResponse>("/api/v1/auth/providers");
  }

  async listLibraries(): Promise<Library[]> {
    return this.requestJson<Library[]>("/api/v1/libraries");
  }

  async createLibrary(request: { name: string; calibre_db_path: string }): Promise<Library> {
    return this.requestJson<Library>("/api/v1/admin/libraries", {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async deleteLibrary(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/libraries/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async setDefaultLibrary(libraryId: string): Promise<User> {
    return this.requestJson<User>("/api/v1/users/me/library", {
      method: "PATCH",
      body: JSON.stringify({ library_id: libraryId }),
    });
  }

  async getMe(): Promise<User> {
    return this.me();
  }

  async changePassword(current: string, next: string): Promise<void> {
    await this.requestJson<void>("/api/v1/auth/me/password", {
      method: "PATCH",
      body: JSON.stringify({ current_password: current, new_password: next }),
    });
  }

  async listBooks(params: ListBooksParams): Promise<PaginatedResponse<BookSummary>> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value === undefined || value === null || value === "") {
        continue;
      }
      if (Array.isArray(value)) {
        value.forEach((item) => search.append(key, item));
      } else {
        search.set(key, String(value));
      }
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<PaginatedResponse<BookSummary>>(`/api/v1/books${suffix}`);
  }

  async search(params: SearchQuery): Promise<PaginatedResponse<SearchResultItem>> {
    const { semantic, ...searchParams } = params;
    const search = new URLSearchParams();

    for (const [key, value] of Object.entries(searchParams)) {
      if (value === undefined || value === null || value === "") {
        continue;
      }
      if (Array.isArray(value)) {
        value.forEach((item) => search.append(key, item));
      } else {
        search.set(key, String(value));
      }
    }

    const suffix = search.toString() ? `?${search.toString()}` : "";
    const path = semantic ? "/api/v1/search/semantic" : "/api/v1/search";
    return this.requestJson<PaginatedResponse<SearchResultItem>>(`${path}${suffix}`);
  }

  async searchSuggestions(q: string, limit = 5): Promise<SearchSuggestionsResponse> {
    const search = new URLSearchParams();
    search.set("q", q);
    search.set("limit", String(limit));

    return this.requestJson<SearchSuggestionsResponse>(
      `/api/v1/search/suggestions?${search.toString()}`,
    );
  }

  async getSearchStatus(): Promise<SearchStatusResponse> {
    return this.requestJson<SearchStatusResponse>("/api/v1/system/search-status");
  }

  async getBook(id: string): Promise<Book> {
    return this.requestJson<Book>(`/api/v1/books/${encodeURIComponent(id)}`);
  }

  async setBookReadState(id: string, isRead: boolean): Promise<void> {
    await this.requestJson<void>(`/api/v1/books/${encodeURIComponent(id)}/read`, {
      method: "POST",
      body: JSON.stringify({ is_read: isRead }),
    });
  }

  async setBookArchivedState(id: string, isArchived: boolean): Promise<void> {
    await this.requestJson<void>(`/api/v1/books/${encodeURIComponent(id)}/archive`, {
      method: "POST",
      body: JSON.stringify({ is_archived: isArchived }),
    });
  }

  async lookupBookMetadata(
    id: string,
    source: "openlibrary" | "googlebooks" = "openlibrary",
  ): Promise<MetadataLookupResponse> {
    const search = new URLSearchParams();
    search.set("source", source);
    return this.requestJson<MetadataLookupResponse>(
      `/api/v1/books/${encodeURIComponent(id)}/metadata-lookup?${search.toString()}`,
    );
  }

  async uploadBook(file: File, metadata?: object): Promise<Book> {
    const form = new FormData();
    form.append("file", file);
    if (metadata) {
      form.append("metadata", JSON.stringify(metadata));
    }
    return this.requestJson<Book>("/api/v1/books", {
      method: "POST",
      body: form,
    });
  }

  async patchBook(id: string, patch: object): Promise<Book> {
    return this.requestJson<Book>(`/api/v1/books/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    });
  }

  async mergeBook(id: string, duplicateId: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/books/${encodeURIComponent(id)}/merge`, {
      method: "POST",
      body: JSON.stringify({ duplicate_id: duplicateId }),
    });
  }

  async listCustomColumns(): Promise<CustomColumn[]> {
    return this.requestJson<CustomColumn[]>("/api/v1/books/custom-columns");
  }

  async createCustomColumn(payload: {
    name: string;
    label: string;
    column_type: CustomColumnType;
    is_multiple: boolean;
  }): Promise<CustomColumn> {
    return this.requestJson<CustomColumn>("/api/v1/books/custom-columns", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  }

  async deleteCustomColumn(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/books/custom-columns/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async getBookCustomValues(id: string): Promise<BookCustomValue[]> {
    return this.requestJson<BookCustomValue[]>(
      `/api/v1/books/${encodeURIComponent(id)}/custom-values`,
    );
  }

  async patchBookCustomValues(id: string, values: BookCustomValuePatch[]): Promise<void> {
    await this.requestJson<void>(`/api/v1/books/${encodeURIComponent(id)}/custom-values`, {
      method: "PATCH",
      body: JSON.stringify(values),
    });
  }

  async deleteBook(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/books/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async classifyBook(bookId: string): Promise<ClassifyResult> {
    return this.requestJson<ClassifyResult>(`/api/v1/books/${encodeURIComponent(bookId)}/classify`);
  }

  async confirmTags(bookId: string, confirm: string[], reject: string[]): Promise<Book> {
    return this.requestJson<Book>(`/api/v1/books/${encodeURIComponent(bookId)}/tags/confirm`, {
      method: "POST",
      body: JSON.stringify({ confirm, reject }),
    });
  }

  async confirmAllTags(bookId: string): Promise<Book> {
    return this.requestJson<Book>(`/api/v1/books/${encodeURIComponent(bookId)}/tags/confirm-all`, {
      method: "POST",
    });
  }

  async searchTags(q: string, limit = 20): Promise<TagLookupItem[]> {
    const search = new URLSearchParams();
    if (q.trim()) {
      search.set("q", q.trim());
    }
    search.set("limit", String(limit));
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<TagLookupItem[]>(`/api/v1/admin/tags${suffix}`);
  }

  async validateBook(bookId: string): Promise<ValidationResult> {
    return this.requestJson<ValidationResult>(`/api/v1/books/${encodeURIComponent(bookId)}/validate`);
  }

  async deriveBook(bookId: string): Promise<DeriveResult> {
    return this.requestJson<DeriveResult>(`/api/v1/books/${encodeURIComponent(bookId)}/derive`);
  }

  async listChapters(bookId: string): Promise<BookChapters> {
    return this.requestJson<BookChapters>(`/api/v1/books/${encodeURIComponent(bookId)}/chapters`);
  }

  async getBookText(bookId: string, chapter?: number): Promise<BookText> {
    const search = new URLSearchParams();
    if (chapter !== undefined) {
      search.set("chapter", String(chapter));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<BookText>(`/api/v1/books/${encodeURIComponent(bookId)}/text${suffix}`);
  }

  async listDownloadHistory(params: {
    page?: number;
    page_size?: number;
  } = {}): Promise<PaginatedResponse<DownloadHistoryItem>> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value === undefined || value === null || value === "") {
        continue;
      }
      search.set(key, String(value));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<PaginatedResponse<DownloadHistoryItem>>(`/api/v1/books/downloads${suffix}`);
  }

  async getLlmHealth(): Promise<LlmHealth> {
    return this.requestJson<LlmHealth>("/api/v1/llm/health");
  }

  async getReadingProgress(id: string): Promise<ReadingProgress | null> {
    try {
      return await this.requestJson<ReadingProgress>(
        `/api/v1/reading-progress/${encodeURIComponent(id)}`,
      );
    } catch (error) {
      const apiError = error as ApiError;
      if (apiError?.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async patchReadingProgress(id: string, patch: ReadingProgressPatch): Promise<ReadingProgress> {
    return this.requestJson<ReadingProgress>(`/api/v1/reading-progress/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    });
  }

  async listUsers(): Promise<AdminUser[]> {
    return this.requestJson<AdminUser[]>("/api/v1/admin/users");
  }

  async listUserTagRestrictions(userId: string): Promise<UserTagRestriction[]> {
    return this.requestJson<UserTagRestriction[]>(
      `/api/v1/admin/users/${encodeURIComponent(userId)}/tag-restrictions`,
    );
  }

  async setUserTagRestriction(
    userId: string,
    payload: { tag_id: string; mode: "allow" | "block" },
  ): Promise<void> {
    await this.requestJson<void>(
      `/api/v1/admin/users/${encodeURIComponent(userId)}/tag-restrictions`,
      {
        method: "POST",
        body: JSON.stringify(payload),
      },
    );
  }

  async deleteUserTagRestriction(userId: string, tagId: string): Promise<void> {
    await this.requestJson<void>(
      `/api/v1/admin/users/${encodeURIComponent(userId)}/tag-restrictions/${encodeURIComponent(tagId)}`,
      {
        method: "DELETE",
      },
    );
  }

  async createUser(request: AdminUserCreateRequest): Promise<AdminUser> {
    return this.requestJson<AdminUser>("/api/v1/admin/users", {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async updateUser(id: string, request: AdminUserUpdateRequest): Promise<AdminUser> {
    return this.requestJson<AdminUser>(`/api/v1/admin/users/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(request),
    });
  }

  async deleteUser(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/users/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async resetUserPassword(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/users/${encodeURIComponent(id)}/reset-password`, {
      method: "POST",
    });
  }

  async listRoles(): Promise<Role[]> {
    return this.requestJson<Role[]>("/api/v1/admin/roles");
  }

  async listJobs(params: {
    status?: string;
    job_type?: string;
    page?: number;
    page_size?: number;
  } = {}): Promise<PaginatedResponse<AdminJob>> {
    return this.listAdminJobs(params);
  }

  async listAdminJobs(params: {
    status?: string;
    job_type?: string;
    page?: number;
    page_size?: number;
  } = {}): Promise<PaginatedResponse<AdminJob>> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value === undefined || value === null || value === "") {
        continue;
      }
      search.set(key, String(value));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<PaginatedResponse<AdminJob>>(`/api/v1/admin/jobs${suffix}`);
  }

  async cancelJob(id: string): Promise<void> {
    await this.cancelAdminJob(id);
  }

  async cancelAdminJob(jobId: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/jobs/${encodeURIComponent(jobId)}`, {
      method: "DELETE",
    });
  }

  async getSystemStats(): Promise<SystemStats> {
    return this.requestJson<SystemStats>("/api/v1/admin/system");
  }

  async listKoboDevices(): Promise<KoboDevice[]> {
    return this.requestJson<KoboDevice[]>("/api/v1/admin/kobo-devices");
  }

  async revokeKoboDevice(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/kobo-devices/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async startBulkImport(request: BulkImportRequest): Promise<BulkImportResponse> {
    if (request.source === "upload" && request.file) {
      const form = new FormData();
      form.append("source", request.source);
      form.append("file", request.file);
      form.append("dry_run", String(Boolean(request.dry_run)));
      return this.requestJson<BulkImportResponse>("/api/v1/admin/import/bulk", {
        method: "POST",
        body: form,
      });
    }

    return this.requestJson<BulkImportResponse>("/api/v1/admin/import/bulk", {
      method: "POST",
      body: JSON.stringify({
        source: request.source,
        path: request.path,
        dry_run: Boolean(request.dry_run),
      }),
    });
  }

  async getImportStatus(id: string): Promise<ImportStatus> {
    return this.requestJson<ImportStatus>(`/api/v1/admin/import/${encodeURIComponent(id)}`);
  }

  async listShelves(): Promise<Shelf[]> {
    return this.requestJson<Shelf[]>("/api/v1/shelves");
  }

  async createShelf(request: { name: string; is_public: boolean }): Promise<Shelf> {
    return this.requestJson<Shelf>("/api/v1/shelves", {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async deleteShelf(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/shelves/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async addBookToShelf(shelfId: string, bookId: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/shelves/${encodeURIComponent(shelfId)}/books`, {
      method: "POST",
      body: JSON.stringify({ book_id: bookId }),
    });
  }

  async removeBookFromShelf(shelfId: string, bookId: string): Promise<void> {
    await this.requestJson<void>(
      `/api/v1/shelves/${encodeURIComponent(shelfId)}/books/${encodeURIComponent(bookId)}`,
      { method: "DELETE" },
    );
  }

  async listShelfBooks(
    shelfId: string,
    params: { page?: number; page_size?: number } = {},
  ): Promise<PaginatedResponse<BookSummary>> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value === undefined || value === null || value === "") {
        continue;
      }
      search.set(key, String(value));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<PaginatedResponse<BookSummary>>(
      `/api/v1/shelves/${encodeURIComponent(shelfId)}/books${suffix}`,
    );
  }

  coverUrl(bookId: string): string {
    return this.url(`/api/v1/books/${encodeURIComponent(bookId)}/cover`);
  }

  downloadUrl(bookId: string, format: string): string {
    return this.url(
      `/api/v1/books/${encodeURIComponent(bookId)}/formats/${encodeURIComponent(format)}/download`,
    );
  }

  streamUrl(bookId: string, format: string): string {
    return this.url(
      `/api/v1/books/${encodeURIComponent(bookId)}/formats/${encodeURIComponent(format)}/stream`,
    );
  }

  private url(path: string): string {
    return `${this.baseUrl.replace(/\/$/, "")}${path}`;
  }

  private async requestJson<T>(
    path: string,
    init: RequestInit = {},
    options: {
      retryOnUnauthorized?: boolean;
      authorizationToken?: string | null;
      notifyUnauthorized?: boolean;
    } = {},
  ): Promise<T> {
    const headers = new Headers(init.headers);
    const token = options.authorizationToken ?? this.getToken();
    if (token) {
      headers.set("Authorization", `Bearer ${token}`);
    }
    if (!headers.has("Content-Type") && !(init.body instanceof FormData)) {
      headers.set("Content-Type", "application/json");
    }

    const response = await fetch(this.url(path), {
      ...init,
      headers,
    });

    if (response.status === 401) {
      if (options.retryOnUnauthorized !== false) {
        const retryToken = await this.tryRefreshToken();
        if (retryToken) {
          return this.requestJson<T>(path, init, {
            retryOnUnauthorized: false,
            authorizationToken: retryToken,
            notifyUnauthorized: options.notifyUnauthorized,
          });
        }
      }
      if (options.notifyUnauthorized !== false) {
        this.onUnauthorized();
      }
    }

    if (!response.ok) {
      throw await this.toApiError(response);
    }

    if (response.status === 204) {
      return undefined as T;
    }

    return (await response.json()) as T;
  }

  private async tryRefreshToken(): Promise<string | null> {
    const refreshToken = this.getRefreshToken();
    if (!refreshToken) {
      return null;
    }

    try {
      const refreshed = await this.refresh(refreshToken);
      return refreshed.access_token;
    } catch {
      return null;
    }
  }

  private getRefreshToken(): string | null {
    return this.options.getRefreshToken?.() ?? this.refreshTokenCache;
  }

  private rememberRefreshToken(refreshToken: string): void {
    this.refreshTokenCache = refreshToken;
  }

  private async toApiError(response: Response): Promise<ApiError> {
    let details: unknown;
    try {
      details = await response.json();
    } catch {
      details = undefined;
    }

    return {
      message: response.statusText || "Request failed",
      status: response.status,
      details,
    };
  }
}

/**
 * ApiClient — isomorphic HTTP client for the xcalibre-server backend.
 *
 * Used by both the web app (React + Vite) and the mobile app (Expo).
 * Wraps every `fetch` call with:
 * - Automatic `Authorization: Bearer <token>` injection
 * - Silent token refresh (single retry) when a 401 is received
 * - `onUnauthorized` callback invocation when refresh also fails (triggers logout)
 * - `Content-Type: application/json` for non-multipart bodies
 * - Error conversion: non-2xx responses throw an {@link ApiError}
 *
 * On mobile the `getToken` callback reads from Expo SecureStore; on web it
 * reads from in-memory state maintained by the auth context.
 *
 * All methods are async and throw {@link ApiError} on non-2xx responses.
 * Callers should catch and handle API errors; never surface raw error text to users.
 *
 * URL helpers (`coverUrl`, `downloadUrl`, `streamUrl`) return absolute URLs
 * by prepending `baseUrl` — pass these directly to `expo-image` or `expo-file-system`.
 */
import type {
  ApiError,
  AdminTag,
  AdminAuthor,
  AdminTagWithCount,
  AdminJob,
  ApiToken,
  ApplyMetadataBody,
  AdminUser,
  AdminUserCreateRequest,
  AdminUserUpdateRequest,
  AuthorDetail,
  AuthorProfilePatch,
  AuthProvidersResponse,
  BulkImportRequest,
  BulkImportResponse,
  CollectionBooksRequest,
  CollectionCreateRequest,
  CollectionDetail,
  CollectionSummary,
  CollectionUpdateRequest,
  Book,
  BookAnnotation,
  BookChapters,
  MetadataLookupResponse,
  MetadataCandidate,
  MergeAuthorResponse,
  MergeTagResponse,
  BookSummary,
  BookText,
  AuthSession,
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
  ScheduledTask,
  ScheduledTaskCreateRequest,
  ScheduledTaskPatchRequest,
  Webhook,
  WebhookCreateRequest,
  WebhookTestResponse,
  WebhookUpdateRequest,
  UpdateCheckResponse,
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
  ReadingImportJob,
  ReadingImportResponse,
  CreateBookAnnotationRequest,
  CreateTokenRequest,
  CreateTokenResponse,
  PatchBookAnnotationRequest,
  SystemStats,
  UserStats,
  Role,
  RefreshResponse,
  RegisterRequest,
  User,
  UserTagRestriction,
  ValidationResult,
} from "./types";

/**
 * Optional callbacks passed to the {@link ApiClient} constructor.
 * Used by the mobile auth layer to keep Expo SecureStore up to date after
 * a silent token refresh.
 */
type ClientOptions = {
  /**
   * Returns the stored refresh token.
   * On mobile this reads from Expo SecureStore (`"refresh_token"` key).
   */
  getRefreshToken?: () => string | null;
  /**
   * Called after a successful token refresh with the new token pair.
   * On mobile this persists both tokens back to Expo SecureStore.
   */
  onRefreshTokens?: (tokens: RefreshResponse) => void;
};

/** Returns true when `value` should be serialized into a query string parameter. */
function isPresentParam(value: unknown): boolean {
  return !(value === undefined || value === null || (typeof value === "string" && value.length === 0));
}

/**
 * HTTP client for the xcalibre-server REST API.
 *
 * @example
 * ```ts
 * const client = new ApiClient(
 *   "https://mylibre.example.com",
 *   () => SecureStore.getItem("access_token"),
 *   () => router.replace("/login"),
 *   {
 *     getRefreshToken: () => SecureStore.getItem("refresh_token"),
 *     onRefreshTokens: (tokens) => {
 *       SecureStore.setItem("access_token", tokens.access_token);
 *       SecureStore.setItem("refresh_token", tokens.refresh_token);
 *     },
 *   },
 * );
 * ```
 */
export class ApiClient {
  // In-memory cache so that a refresh_token received during login can be used
  // for silent renewal even when no persistent storage callback is provided.
  private refreshTokenCache: string | null = null;

  /**
   * @param baseUrl - Root URL of the xcalibre-server server, e.g. "https://mylibre.example.com".
   *   Trailing slashes are stripped automatically.
   * @param getToken - Returns the current access token or null when unauthenticated.
   *   Called before every request.
   * @param onUnauthorized - Called when a 401 cannot be recovered by refreshing.
   *   Should navigate the user to the login screen.
   * @param options - Optional refresh-token callbacks.
   */
  constructor(
    private readonly baseUrl: string,
    private readonly getToken: () => string | null,
    private readonly onUnauthorized: () => void,
    private readonly options: ClientOptions = {},
  ) {}

  /**
   * POST /api/v1/auth/login
   *
   * Authenticates with username + password credentials.
   * Returns an {@link AuthSession} on success, or a
   * {@link LoginTotpRequiredResponse} when the account has TOTP enabled.
   * Callers must discriminate the union: `"totp_required" in response`.
   *
   * Does **not** throw on 401 — the caller should surface credential errors.
   * Does NOT set the auth header (credentials are the payload).
   */
  async login(req: LoginRequest): Promise<LoginResponse> {
    const response = await this.requestJson<LoginResponse>(
      "/api/v1/auth/login",
      {
        method: "POST",
        body: JSON.stringify(req),
      },
      { retryOnUnauthorized: false, notifyUnauthorized: false },
    );
    if (!("totp_required" in response)) {
      this.rememberRefreshToken(response.refresh_token);
    }
    return response;
  }

  /**
   * POST /api/v1/auth/register
   * Creates a new user account. Returns the created {@link User}.
   * Throws if registration is disabled on the server or the username/email is taken.
   */
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

  /**
   * POST /api/v1/auth/refresh
   *
   * Exchanges a refresh token for a new token pair.
   * Called automatically by the internal retry logic when a 401 is received.
   * Also invokes `options.onRefreshTokens` so callers can persist the new tokens.
   * Both the old refresh token and the old access token are invalidated on success.
   */
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

  /**
   * POST /api/v1/auth/logout
   * Revokes the provided refresh token on the server and clears the in-memory cache.
   * On mobile the caller should also clear Expo SecureStore after this resolves.
   */
  async logout(refreshToken: string): Promise<void> {
    await this.requestJson<void>("/api/v1/auth/logout", {
      method: "POST",
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    this.refreshTokenCache = null;
  }

  /**
   * GET /api/v1/auth/me
   * Returns the currently authenticated user. Alias: {@link getMe}.
   */
  async me(): Promise<User> {
    return this.requestJson<User>("/api/v1/auth/me");
  }

  /**
   * GET /api/v1/auth/providers
   * Returns which OAuth providers (Google, GitHub) are configured on the server.
   * Used to conditionally render OAuth sign-in buttons on the login screen.
   */
  async getAuthProviders(): Promise<AuthProvidersResponse> {
    return this.requestJson<AuthProvidersResponse>("/api/v1/auth/providers");
  }

  /** GET /api/v1/libraries — Returns all libraries the current user has access to. */
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

  /**
   * GET /api/v1/users/me/stats
   * Returns aggregated reading statistics for the current user.
   * Used by the mobile Stats screen and the Profile tab summary row.
   */
  async getUserStats(): Promise<UserStats> {
    return this.requestJson<UserStats>("/api/v1/users/me/stats");
  }

  async changePassword(current: string, next: string): Promise<void> {
    await this.requestJson<void>("/api/v1/auth/me/password", {
      method: "PATCH",
      body: JSON.stringify({ current_password: current, new_password: next }),
    });
  }

  /**
   * GET /api/v1/auth/totp/setup
   * Initiates TOTP setup for the current user.
   * Returns a base32 secret and an `otpauth://` URI suitable for rendering as a QR code.
   * Requires confirmation via {@link confirmTotp} before TOTP is activated.
   */
  async setupTotp(): Promise<{ secret_base32: string; otpauth_uri: string }> {
    return this.requestJson<{ secret_base32: string; otpauth_uri: string }>("/api/v1/auth/totp/setup", {
      method: "GET",
    });
  }

  /**
   * POST /api/v1/auth/totp/confirm
   * Activates TOTP for the current user by verifying the first code from the authenticator app.
   * Returns a list of one-time backup codes that the user should store securely.
   */
  async confirmTotp(code: string): Promise<{ backup_codes: string[] }> {
    return this.requestJson<{ backup_codes: string[] }>("/api/v1/auth/totp/confirm", {
      method: "POST",
      body: JSON.stringify({ code }),
    });
  }

  async disableTotp(password: string): Promise<void> {
    await this.requestJson<void>("/api/v1/auth/totp/disable", {
      method: "POST",
      body: JSON.stringify({ password }),
    });
  }

  async listWebhooks(): Promise<Webhook[]> {
    return this.requestJson<Webhook[]>("/api/v1/users/me/webhooks");
  }

  async createWebhook(request: WebhookCreateRequest): Promise<Webhook> {
    return this.requestJson<Webhook>("/api/v1/users/me/webhooks", {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async updateWebhook(id: string, request: WebhookUpdateRequest): Promise<Webhook> {
    return this.requestJson<Webhook>(`/api/v1/users/me/webhooks/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(request),
    });
  }

  async deleteWebhook(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/users/me/webhooks/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async testWebhook(id: string): Promise<WebhookTestResponse> {
    return this.requestJson<WebhookTestResponse>(
      `/api/v1/users/me/webhooks/${encodeURIComponent(id)}/test`,
      {
        method: "POST",
      },
    );
  }

  /**
   * POST /api/v1/auth/totp/verify
   * Completes the TOTP challenge step of login.
   * @param token - The temporary `totp_token` received in {@link LoginTotpRequiredResponse}.
   * @param code - The 6-digit code from the user's authenticator app.
   * @returns A full {@link AuthSession} on success.
   */
  async verifyTotp(token: string, code: string): Promise<AuthSession> {
    return this.requestJson<AuthSession>("/api/v1/auth/totp/verify", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ code }),
    }, {
      authorizationToken: token,
      retryOnUnauthorized: false,
      notifyUnauthorized: false,
    });
  }

  /**
   * POST /api/v1/auth/totp/verify-backup
   * Like {@link verifyTotp} but accepts a single-use backup recovery code instead of a TOTP.
   * The consumed backup code is invalidated after use.
   */
  async verifyTotpBackup(token: string, code: string): Promise<AuthSession> {
    return this.requestJson<AuthSession>("/api/v1/auth/totp/verify-backup", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ code }),
    }, {
      authorizationToken: token,
      retryOnUnauthorized: false,
      notifyUnauthorized: false,
    });
  }

  /**
   * GET /api/v1/books
   * Returns a paginated list of books matching the given filters.
   * Used by the library grid (infinite scroll) and offline sync (via `since`).
   * All params are serialized as query string entries; arrays use repeated keys.
   */
  async listBooks(params: ListBooksParams): Promise<PaginatedResponse<BookSummary>> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (!isPresentParam(value)) {
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

  async listInProgress(): Promise<BookSummary[]> {
    return this.requestJson<BookSummary[]>("/api/v1/books/in-progress");
  }

  /**
   * GET /api/v1/search or GET /api/v1/search/semantic
   *
   * Searches the library using full-text (Meilisearch/SQLite FTS5) by default,
   * or vector semantic search when `params.semantic === true`.
   * The `semantic` flag is stripped before building the query string; it only
   * controls which endpoint path is used.
   *
   * @returns Paginated {@link SearchResultItem} list. Semantic results include a `score` field.
   */
  async search(params: SearchQuery): Promise<PaginatedResponse<SearchResultItem>> {
    const { semantic, ...searchParams } = params;
    const search = new URLSearchParams();

    for (const [key, value] of Object.entries(searchParams)) {
      if (!isPresentParam(value)) {
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

  async listCollections(): Promise<CollectionSummary[]> {
    return this.requestJson<CollectionSummary[]>("/api/v1/collections");
  }

  async getCollection(id: string): Promise<CollectionDetail> {
    return this.requestJson<CollectionDetail>(`/api/v1/collections/${encodeURIComponent(id)}`);
  }

  async createCollection(request: CollectionCreateRequest): Promise<CollectionSummary> {
    return this.requestJson<CollectionSummary>("/api/v1/collections", {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async updateCollection(id: string, request: CollectionUpdateRequest): Promise<CollectionSummary> {
    return this.requestJson<CollectionSummary>(`/api/v1/collections/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(request),
    });
  }

  async deleteCollection(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/collections/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async addBooksToCollection(id: string, bookIds: string[]): Promise<void> {
    await this.requestJson<void>(`/api/v1/collections/${encodeURIComponent(id)}/books`, {
      method: "POST",
      body: JSON.stringify({ book_ids: bookIds } satisfies CollectionBooksRequest),
    });
  }

  async removeBookFromCollection(id: string, bookId: string): Promise<void> {
    await this.requestJson<void>(
      `/api/v1/collections/${encodeURIComponent(id)}/books/${encodeURIComponent(bookId)}`,
      { method: "DELETE" },
    );
  }

  /**
   * GET /api/v1/books/:id
   * Returns the full {@link Book} record including formats, tags, and identifiers.
   */
  async getBook(id: string): Promise<Book> {
    return this.requestJson<Book>(`/api/v1/books/${encodeURIComponent(id)}`);
  }

  async getAuthor(
    id: string,
    params: { page?: number; page_size?: number } = {},
  ): Promise<AuthorDetail> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (!isPresentParam(value)) {
        continue;
      }
      search.set(key, String(value));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<AuthorDetail>(`/api/v1/authors/${encodeURIComponent(id)}${suffix}`);
  }

  async patchAuthor(id: string, patch: AuthorProfilePatch): Promise<AuthorDetail> {
    return this.requestJson<AuthorDetail>(`/api/v1/authors/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    });
  }

  async uploadAuthorPhoto(id: string, photo: File): Promise<AuthorDetail> {
    const form = new FormData();
    form.append("photo", photo);
    return this.requestJson<AuthorDetail>(`/api/v1/authors/${encodeURIComponent(id)}/photo`, {
      method: "POST",
      body: form,
    });
  }

  async listAdminAuthors(params: {
    q?: string;
    page?: number;
    page_size?: number;
  } = {}): Promise<PaginatedResponse<AdminAuthor>> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (!isPresentParam(value)) {
        continue;
      }
      search.set(key, String(value));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<PaginatedResponse<AdminAuthor>>(`/api/v1/admin/authors${suffix}`);
  }

  async listApiTokens(): Promise<ApiToken[]> {
    return this.requestJson<ApiToken[]>("/api/v1/admin/tokens");
  }

  async createApiToken(request: CreateTokenRequest): Promise<CreateTokenResponse> {
    return this.requestJson<CreateTokenResponse>("/api/v1/admin/tokens", {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async deleteApiToken(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/tokens/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async mergeAuthor(id: string, intoAuthorId: string): Promise<MergeAuthorResponse> {
    return this.requestJson<MergeAuthorResponse>(`/api/v1/admin/authors/${encodeURIComponent(id)}/merge`, {
      method: "POST",
      body: JSON.stringify({ into_author_id: intoAuthorId }),
    });
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

  async searchBookMetadata(bookId: string, q: string): Promise<MetadataCandidate[]> {
    const search = new URLSearchParams();
    search.set("q", q);
    return this.requestJson<MetadataCandidate[]>(
      `/api/v1/books/${encodeURIComponent(bookId)}/metadata/search?${search.toString()}`,
    );
  }

  async applyBookMetadata(bookId: string, body: ApplyMetadataBody): Promise<Book> {
    return this.requestJson<Book>(`/api/v1/books/${encodeURIComponent(bookId)}/metadata/apply`, {
      method: "POST",
      body: JSON.stringify(body),
    });
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
    const response = await this.listAdminTags({
      q,
      page: 1,
      page_size: limit,
    });
    return response.items.map((item) => ({ id: item.id, name: item.name }));
  }

  async listAdminTags(params: {
    q?: string;
    page?: number;
    page_size?: number;
  } = {}): Promise<PaginatedResponse<AdminTagWithCount>> {
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (!isPresentParam(value)) {
        continue;
      }
      search.set(key, String(value));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<PaginatedResponse<AdminTagWithCount>>(`/api/v1/admin/tags${suffix}`);
  }

  async renameAdminTag(id: string, name: string): Promise<AdminTag> {
    return this.requestJson<AdminTag>(`/api/v1/admin/tags/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify({ name }),
    });
  }

  async deleteAdminTag(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/tags/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async mergeAdminTag(id: string, intoTagId: string): Promise<MergeTagResponse> {
    return this.requestJson<MergeTagResponse>(`/api/v1/admin/tags/${encodeURIComponent(id)}/merge`, {
      method: "POST",
      body: JSON.stringify({ into_tag_id: intoTagId }),
    });
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
      if (!isPresentParam(value)) {
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

  /**
   * GET /api/v1/books/:id/progress
   * Returns the current reading position for the authenticated user, or null if
   * no progress has been recorded yet (404 is swallowed and returned as null).
   */
  async getReadingProgress(id: string): Promise<ReadingProgress | null> {
    try {
      return await this.requestJson<ReadingProgress>(
        `/api/v1/books/${encodeURIComponent(id)}/progress`,
      );
    } catch (error) {
      const apiError = error as ApiError;
      if (apiError?.status === 404) {
        return null;
      }
      throw error;
    }
  }

  /**
   * PATCH /api/v1/books/:id/progress
   * Creates or updates the reading position for the authenticated user.
   * Called by both readers (EPUB and PDF) after a debounced position change.
   */
  async patchReadingProgress(id: string, patch: ReadingProgressPatch): Promise<ReadingProgress> {
    return this.requestJson<ReadingProgress>(`/api/v1/books/${encodeURIComponent(id)}/progress`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    });
  }

  /**
   * GET /api/v1/books/:id/annotations
   * Returns all annotations the current user has created for this book.
   * Called on EPUB reader mount to populate the highlight/note overlays.
   */
  async listBookAnnotations(bookId: string): Promise<BookAnnotation[]> {
    return this.requestJson<BookAnnotation[]>(`/api/v1/books/${encodeURIComponent(bookId)}/annotations`);
  }

  /**
   * POST /api/v1/books/:id/annotations
   * Creates a new annotation. The EPUB reader uses optimistic updates: a
   * temporary annotation is inserted locally before this resolves, then
   * replaced with the server-assigned ID on success.
   */
  async createBookAnnotation(bookId: string, payload: CreateBookAnnotationRequest): Promise<BookAnnotation> {
    return this.requestJson<BookAnnotation>(`/api/v1/books/${encodeURIComponent(bookId)}/annotations`, {
      method: "POST",
      body: JSON.stringify(payload),
    });
  }

  /**
   * PATCH /api/v1/books/:id/annotations/:annotationId
   * Updates an annotation's color and/or note text. Supports optimistic updates
   * in the EPUB reader: the local state is patched immediately, then reconciled
   * with the server response (or rolled back on failure).
   */
  async patchBookAnnotation(
    bookId: string,
    annotationId: string,
    payload: PatchBookAnnotationRequest,
  ): Promise<BookAnnotation> {
    return this.requestJson<BookAnnotation>(
      `/api/v1/books/${encodeURIComponent(bookId)}/annotations/${encodeURIComponent(annotationId)}`,
      {
        method: "PATCH",
        body: JSON.stringify(payload),
      },
    );
  }

  /**
   * DELETE /api/v1/books/:id/annotations/:annotationId
   * Deletes an annotation. The EPUB reader removes it from local state
   * optimistically and re-adds it if the request fails.
   */
  async deleteBookAnnotation(bookId: string, annotationId: string): Promise<void> {
    await this.requestJson<void>(
      `/api/v1/books/${encodeURIComponent(bookId)}/annotations/${encodeURIComponent(annotationId)}`,
      {
        method: "DELETE",
      },
    );
  }

  async listUsers(): Promise<AdminUser[]> {
    return this.requestJson<AdminUser[]>("/api/v1/admin/users");
  }

  async listScheduledTasks(): Promise<ScheduledTask[]> {
    return this.requestJson<ScheduledTask[]>("/api/v1/admin/scheduled-tasks");
  }

  async createScheduledTask(request: ScheduledTaskCreateRequest): Promise<ScheduledTask> {
    return this.requestJson<ScheduledTask>("/api/v1/admin/scheduled-tasks", {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async updateScheduledTask(id: string, request: ScheduledTaskPatchRequest): Promise<ScheduledTask> {
    return this.requestJson<ScheduledTask>(`/api/v1/admin/scheduled-tasks/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(request),
    });
  }

  async deleteScheduledTask(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/scheduled-tasks/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async getUpdateCheck(): Promise<UpdateCheckResponse | null> {
    try {
      return await this.requestJson<UpdateCheckResponse>("/api/v1/admin/update-check");
    } catch (error) {
      const apiError = error as ApiError;
      if (apiError?.status === 503) {
        return null;
      }
      throw error;
    }
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

  async disableUserTotp(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/admin/users/${encodeURIComponent(id)}/totp/disable`, {
      method: "POST",
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
      if (!isPresentParam(value)) {
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

  async startGoodreadsImport(file: File): Promise<ReadingImportResponse> {
    const form = new FormData();
    form.append("file", file);
    return this.requestJson<ReadingImportResponse>("/api/v1/users/me/import/goodreads", {
      method: "POST",
      body: form,
    });
  }

  async startStorygraphImport(file: File): Promise<ReadingImportResponse> {
    const form = new FormData();
    form.append("file", file);
    return this.requestJson<ReadingImportResponse>("/api/v1/users/me/import/storygraph", {
      method: "POST",
      body: form,
    });
  }

  async getReadingImportStatus(id: string): Promise<ReadingImportJob> {
    return this.requestJson<ReadingImportJob>(`/api/v1/users/me/import/${encodeURIComponent(id)}`);
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
      if (!isPresentParam(value)) {
        continue;
      }
      search.set(key, String(value));
    }
    const suffix = search.toString() ? `?${search.toString()}` : "";
    return this.requestJson<PaginatedResponse<BookSummary>>(
      `/api/v1/shelves/${encodeURIComponent(shelfId)}/books${suffix}`,
    );
  }

  /**
   * Returns the absolute URL for a book's cover image.
   * Pass this directly to `expo-image`'s `source.uri` prop.
   * The backend serves the cover at `/api/v1/books/:id/cover`.
   */
  coverUrl(bookId: string): string {
    return this.url(`/api/v1/books/${encodeURIComponent(bookId)}/cover`);
  }

  /**
   * Returns the absolute URL for downloading a specific format file.
   * Used by the download queue: passed to `FileSystem.createDownloadResumable`
   * along with a `Authorization: Bearer` header.
   */
  downloadUrl(bookId: string, format: string): string {
    return this.url(
      `/api/v1/books/${encodeURIComponent(bookId)}/formats/${encodeURIComponent(format)}/download`,
    );
  }

  /**
   * Returns the absolute URL for streaming a specific format for online reading.
   * Used when no local file is available and the reader should stream from the server.
   */
  streamUrl(bookId: string, format: string): string {
    return this.url(
      `/api/v1/books/${encodeURIComponent(bookId)}/formats/${encodeURIComponent(format)}/stream`,
    );
  }

  private url(path: string): string {
    return `${this.baseUrl.replace(/\/$/, "")}${path}`;
  }

  /**
   * Core HTTP helper used by every public method.
   *
   * Injects the `Authorization: Bearer` header, sets `Content-Type: application/json`
   * for non-FormData bodies, handles the 401 → refresh → retry cycle, and converts
   * non-2xx responses to {@link ApiError}.
   *
   * @param options.retryOnUnauthorized - When false, a 401 is not retried (used for
   *   auth endpoints where a 401 is a credential error, not a token expiry).
   * @param options.authorizationToken - Override the token from `getToken()`; used
   *   during the retry to inject the newly refreshed access token.
   * @param options.notifyUnauthorized - When false, `onUnauthorized` is not called
   *   even if the request ultimately fails with 401 (used for auth endpoints).
   */
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

  /**
   * Attempts a silent token refresh using the stored refresh token.
   * Returns the new access token on success, or null if no refresh token is
   * available or if the refresh request itself fails.
   */
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

  /**
   * Retrieves the refresh token from the options callback (Expo SecureStore on
   * mobile) or falls back to the in-memory cache populated at login time.
   */
  private getRefreshToken(): string | null {
    return this.options.getRefreshToken?.() ?? this.refreshTokenCache;
  }

  /** Caches the refresh token in memory so it can be used without a storage callback. */
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

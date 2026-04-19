import type {
  ApiError,
  AdminJob,
  AdminUser,
  AdminUserCreateRequest,
  AdminUserUpdateRequest,
  BulkImportRequest,
  BulkImportResponse,
  Book,
  BookSummary,
  ListBooksParams,
  LoginRequest,
  LoginResponse,
  PaginatedResponse,
  ImportStatus,
  ReadingProgress,
  ReadingProgressPatch,
  SystemStats,
  Role,
  RefreshResponse,
  RegisterRequest,
  User,
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

  async getBook(id: string): Promise<Book> {
    return this.requestJson<Book>(`/api/v1/books/${encodeURIComponent(id)}`);
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

  async deleteBook(id: string): Promise<void> {
    await this.requestJson<void>(`/api/v1/books/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
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
    await this.requestJson<void>(`/api/v1/admin/jobs/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async getSystemStats(): Promise<SystemStats> {
    return this.requestJson<SystemStats>("/api/v1/admin/system");
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

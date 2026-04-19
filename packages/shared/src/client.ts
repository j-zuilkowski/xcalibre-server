import type {
  ApiError,
  Book,
  BookSummary,
  ListBooksParams,
  LoginRequest,
  LoginResponse,
  PaginatedResponse,
  RefreshResponse,
  RegisterRequest,
  User,
} from "./types";

export class ApiClient {
  constructor(
    private readonly baseUrl: string,
    private readonly getToken: () => string | null,
    private readonly onUnauthorized: () => void,
  ) {}

  async login(req: LoginRequest): Promise<LoginResponse> {
    return this.requestJson<LoginResponse>("/api/v1/auth/login", {
      method: "POST",
      body: JSON.stringify(req),
    });
  }

  async register(req: RegisterRequest): Promise<User> {
    return this.requestJson<User>("/api/v1/auth/register", {
      method: "POST",
      body: JSON.stringify(req),
    });
  }

  async refresh(refreshToken: string): Promise<RefreshResponse> {
    return this.requestJson<RefreshResponse>("/api/v1/auth/refresh", {
      method: "POST",
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
  }

  async logout(refreshToken: string): Promise<void> {
    await this.requestJson<void>("/api/v1/auth/logout", {
      method: "POST",
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
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
  ): Promise<T> {
    const headers = new Headers(init.headers);
    const token = this.getToken();
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
      this.onUnauthorized();
    }

    if (!response.ok) {
      throw await this.toApiError(response);
    }

    if (response.status === 204) {
      return undefined as T;
    }

    return (await response.json()) as T;
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

/**
 * WebhooksPage — personal webhook CRUD and delivery history.
 *
 * Route: /profile/webhooks
 *
 * The form serves dual purpose: creating a new webhook (when `editingWebhook`
 * is null) or editing an existing one (when set).  The signing secret is
 * write-once — it cannot be retrieved or changed after creation, so the edit
 * form hides the secret field and shows a read-only note.
 *
 * Delivery test: "Test" button fires POST /api/v1/webhooks/:id/test and
 * stores the result locally in `testResults[webhookId]`.  The test mutation
 * catches API errors and converts them to a synthetic WebhookTestResponse with
 * `delivered: false` so the UI always gets a displayable result rather than
 * throwing.
 *
 * Delete confirmation uses the shadcn Dialog component with focus trap;
 * `initialFocusRef` points to the Cancel button so keyboard users land there.
 *
 * Webhook status badge logic (`webhookStatusLabel`):
 *   - Disabled   → "Disabled"
 *   - Has error  → "Needs attention"
 *   - Delivered  → "Healthy"
 *   - Enabled    → "Enabled"
 *
 * API calls:
 *   GET    /api/v1/webhooks
 *   POST   /api/v1/webhooks
 *   PATCH  /api/v1/webhooks/:id
 *   DELETE /api/v1/webhooks/:id
 *   POST   /api/v1/webhooks/:id/test
 */
import { useId, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { ApiError, Webhook, WebhookCreateRequest, WebhookEventName, WebhookTestResponse, WebhookUpdateRequest } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { Dialog } from "../../components/ui/Dialog";
import { formatDateTime } from "../admin/admin-utils";
import { ProfileSidebar } from "./ProfileSidebar";

const EVENT_OPTIONS: Array<{ value: WebhookEventName; label: string; description: string }> = [
  { value: "book.added", label: "Book added", description: "Book ingest completed successfully." },
  { value: "book.deleted", label: "Book deleted", description: "A book was deleted from the library." },
  { value: "import.completed", label: "Import completed", description: "A reading-history import finished." },
  { value: "llm_job.completed", label: "LLM job completed", description: "An LLM background job finished." },
  { value: "user.registered", label: "User registered", description: "A new user registration completed." },
];

type WebhookFormState = {
  url: string;
  secret: string;
  events: WebhookEventName[];
  enabled: boolean;
};

const DEFAULT_FORM_STATE: WebhookFormState = {
  url: "",
  secret: "",
  events: ["book.added"],
  enabled: true,
};

function webhookStatusLabel(webhook: Webhook): string {
  if (!webhook.enabled) {
    return "Disabled";
  }
  if (webhook.last_error) {
    return "Needs attention";
  }
  if (webhook.last_delivery_at) {
    return "Healthy";
  }
  return "Enabled";
}

function describeTestResult(result: WebhookTestResponse | null): string {
  if (!result) {
    return "Not tested";
  }
  if (result.delivered) {
    return `Delivered${result.response_status ? ` (${result.response_status})` : ""}`;
  }
  if (result.response_status) {
    return `Failed (${result.response_status})`;
  }
  return result.error ? `Failed (${result.error})` : "Failed";
}

function extractApiErrorMessage(error: unknown): string {
  const apiError = error as ApiError | undefined;
  const details = apiError?.details as { message?: string; error?: string } | undefined;
  return details?.message ?? details?.error ?? apiError?.message ?? "Request failed";
}

/**
 * WebhooksPage renders the webhook management interface: a create/edit form,
 * a delivery history table, and a delete confirmation dialog.
 */
export function WebhooksPage() {
  const queryClient = useQueryClient();
  const [formState, setFormState] = useState<WebhookFormState>(DEFAULT_FORM_STATE);
  const [editingWebhook, setEditingWebhook] = useState<Webhook | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, WebhookTestResponse | null>>({});
  const [deleteWebhook, setDeleteWebhook] = useState<Webhook | null>(null);
  const deleteDialogTitleId = useId();
  const deleteCancelRef = useRef<HTMLButtonElement | null>(null);

  const webhooksQuery = useQuery({
    queryKey: ["profile-webhooks"],
    queryFn: () => apiClient.listWebhooks(),
  });

  const createMutation = useMutation({
    mutationFn: (request: WebhookCreateRequest) => apiClient.createWebhook(request),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["profile-webhooks"] });
      setFormState(DEFAULT_FORM_STATE);
      setEditingWebhook(null);
      setFormError(null);
    },
    onError: (error) => {
      setFormError(extractApiErrorMessage(error));
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, request }: { id: string; request: WebhookUpdateRequest }) =>
      apiClient.updateWebhook(id, request),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["profile-webhooks"] });
      setEditingWebhook(null);
      setFormError(null);
    },
    onError: (error) => {
      setFormError(extractApiErrorMessage(error));
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteWebhook(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["profile-webhooks"] });
    },
  });

  const testMutation = useMutation({
    mutationFn: async (webhook: Webhook) => {
      try {
        return await apiClient.testWebhook(webhook.id);
      } catch (error) {
        const apiError = error as ApiError;
        return {
          delivered: false,
          response_status: apiError.status ?? null,
          error: extractApiErrorMessage(error),
        } satisfies WebhookTestResponse;
      }
    },
    onSuccess: (result, webhook) => {
      setTestResults((current) => ({
        ...current,
        [webhook.id]: result,
      }));
    },
  });

  const webhooks = webhooksQuery.data ?? [];
  const isEditing = editingWebhook !== null;

  function beginCreate() {
    setEditingWebhook(null);
    setFormState(DEFAULT_FORM_STATE);
    setFormError(null);
  }

  function beginEdit(webhook: Webhook) {
    setEditingWebhook(webhook);
    setFormState({
      url: webhook.url,
      secret: "",
      events: webhook.events,
      enabled: webhook.enabled,
    });
    setFormError(null);
  }

  function toggleEvent(eventName: WebhookEventName) {
    setFormState((current) => {
      const events = current.events.includes(eventName)
        ? current.events.filter((value) => value !== eventName)
        : [...current.events, eventName];
      return { ...current, events };
    });
  }

  async function submitForm() {
    const url = formState.url.trim();
    const events = formState.events;

    if (!url) {
      setFormError("Enter a webhook URL.");
      return;
    }
    if (events.length === 0) {
      setFormError("Select at least one event.");
      return;
    }
    if (!isEditing && !formState.secret.trim()) {
      setFormError("Enter a signing secret.");
      return;
    }

    setFormError(null);
    if (editingWebhook) {
      await updateMutation.mutateAsync({
        id: editingWebhook.id,
        request: {
          url,
          events,
          enabled: formState.enabled,
        },
      });
      return;
    }

    await createMutation.mutateAsync({
      url,
      secret: formState.secret.trim(),
      events,
    });
  }

  return (
    <div className="mx-auto flex w-full max-w-7xl flex-col gap-6 lg:flex-row">
      <ProfileSidebar active="webhooks" />

      <main className="min-w-0 flex-1">
        <div className="flex flex-col gap-6">
          <header className="rounded-3xl border border-zinc-800 bg-[radial-gradient(circle_at_top_left,rgba(20,184,166,0.18),transparent_40%),linear-gradient(180deg,rgba(24,24,27,0.92),rgba(24,24,27,0.72))] p-6 shadow-[0_24px_80px_-30px_rgba(15,118,110,0.4)]">
            <p className="text-sm uppercase tracking-[0.2em] text-teal-300">Profile</p>
            <div className="mt-2 flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
              <div>
                <h2 className="text-3xl font-semibold text-zinc-50">Webhooks</h2>
                <p className="mt-2 max-w-2xl text-sm leading-6 text-zinc-400">
                  Deliver library events to your own endpoint with signed JSON payloads.
                </p>
              </div>
              <button
                type="button"
                onClick={beginCreate}
                className="w-fit rounded-full bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950"
              >
                Add webhook
              </button>
            </div>
          </header>

          <section className="rounded-3xl border border-zinc-800 bg-zinc-900/80 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold text-zinc-50">
                  {isEditing ? "Edit webhook" : "Create webhook"}
                </h3>
                <p className="mt-1 text-sm text-zinc-400">
                  HTTPS is required when saving a new webhook. The secret is encrypted at rest.
                </p>
              </div>
              {isEditing ? (
                <button
                  type="button"
                  onClick={beginCreate}
                  className="rounded-full border border-zinc-700 px-3 py-1 text-xs font-medium text-zinc-300"
                >
                  New webhook
                </button>
              ) : null}
            </div>

            <div className="mt-5 grid gap-5 lg:grid-cols-[1fr_360px]">
              <div className="space-y-4">
                <label className="block">
                  <span className="mb-2 block text-sm text-zinc-300">Webhook URL</span>
                  <input
                    value={formState.url}
                    onChange={(event) => setFormState((current) => ({ ...current, url: event.target.value }))}
                    placeholder="https://example.com/hook"
                    className="w-full rounded-xl border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none"
                  />
                </label>

                {!isEditing ? (
                  <label className="block">
                    <span className="mb-2 block text-sm text-zinc-300">Signing secret</span>
                    <input
                      value={formState.secret}
                      onChange={(event) =>
                        setFormState((current) => ({ ...current, secret: event.target.value }))
                      }
                      placeholder="my-secret"
                      type="password"
                      className="w-full rounded-xl border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none"
                    />
                  </label>
                ) : (
                  <div className="rounded-xl border border-zinc-800 bg-zinc-950/70 px-3 py-2 text-sm text-zinc-400">
                    Signing secret cannot be changed here.
                  </div>
                )}

                <div className="rounded-2xl border border-zinc-800 bg-zinc-950/70 p-4">
                  <p className="text-sm font-medium text-zinc-100">Events</p>
                  <div className="mt-4 grid gap-3 sm:grid-cols-2">
                    {EVENT_OPTIONS.map((event) => {
                      const checked = formState.events.includes(event.value);
                      return (
                        <label
                          key={event.value}
                          className={`rounded-xl border px-3 py-3 transition ${
                            checked
                              ? "border-teal-500/40 bg-teal-500/10"
                              : "border-zinc-800 bg-zinc-950/60"
                          }`}
                        >
                          <div className="flex items-start gap-3">
                            <input
                              type="checkbox"
                              checked={checked}
                              onChange={() => toggleEvent(event.value)}
                              className="mt-1"
                            />
                            <span>
                              <span className="block text-sm font-medium text-zinc-50">{event.label}</span>
                              <span className="mt-1 block text-xs text-zinc-400">{event.description}</span>
                            </span>
                          </div>
                        </label>
                      );
                    })}
                  </div>
                </div>
              </div>

              <div className="space-y-4">
                {isEditing ? (
                  <label className="flex items-center justify-between rounded-2xl border border-zinc-800 bg-zinc-950/70 px-4 py-3 text-sm text-zinc-300">
                    <span>Enabled</span>
                    <input
                      type="checkbox"
                      checked={formState.enabled}
                      onChange={(event) =>
                        setFormState((current) => ({ ...current, enabled: event.target.checked }))
                      }
                    />
                  </label>
                ) : (
                  <div className="rounded-2xl border border-zinc-800 bg-zinc-950/70 px-4 py-3 text-sm text-zinc-400">
                    New webhooks start enabled.
                  </div>
                )}

                {formError ? (
                  <div className="rounded-2xl border border-red-900 bg-red-950/60 px-4 py-3 text-sm text-red-200">
                    {formError}
                  </div>
                ) : null}

                <div className="flex flex-wrap gap-3">
                  <button
                    type="button"
                    onClick={() => {
                      void submitForm();
                    }}
                    disabled={createMutation.isPending || updateMutation.isPending}
                    className="rounded-xl bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-60"
                  >
                    {isEditing
                      ? updateMutation.isPending
                        ? "Saving..."
                        : "Save changes"
                      : createMutation.isPending
                        ? "Creating..."
                        : "Create webhook"}
                  </button>
                  <button
                    type="button"
                    onClick={beginCreate}
                    className="rounded-xl border border-zinc-700 px-4 py-2 text-sm text-zinc-200"
                  >
                    Reset
                  </button>
                </div>
              </div>
            </div>
          </section>

          <section className="overflow-hidden rounded-3xl border border-zinc-800 bg-zinc-900/80">
            <table className="min-w-full border-collapse text-left text-sm">
              <thead className="bg-zinc-950/60 text-zinc-400">
                <tr>
                  <th className="px-4 py-3 font-medium">URL</th>
                  <th className="px-4 py-3 font-medium">Events</th>
                  <th className="px-4 py-3 font-medium">Last delivery</th>
                  <th className="px-4 py-3 font-medium">Status</th>
                  <th className="px-4 py-3 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody>
                {webhooks.map((webhook) => {
                  const testResult = testResults[webhook.id] ?? null;
                  return (
                    <tr key={webhook.id} className="border-t border-zinc-800 align-top">
                      <td className="px-4 py-3">
                        <div className="font-medium text-zinc-100">{webhook.url}</div>
                        {webhook.last_error ? (
                          <div className="mt-1 max-w-md text-xs text-red-300">{webhook.last_error}</div>
                        ) : null}
                      </td>
                      <td className="px-4 py-3">
                        <div className="flex flex-wrap gap-2">
                          {webhook.events.map((event) => (
                            <span
                              key={event}
                              className="rounded-full border border-zinc-700 bg-zinc-950/60 px-2 py-1 text-xs text-zinc-300"
                            >
                              {event}
                            </span>
                          ))}
                        </div>
                      </td>
                      <td className="px-4 py-3 text-zinc-300">{formatDateTime(webhook.last_delivery_at)}</td>
                      <td className="px-4 py-3">
                        <span
                          className={`inline-flex rounded-full px-3 py-1 text-xs font-semibold ${
                            webhook.enabled
                              ? webhook.last_error
                                ? "border border-amber-500/30 bg-amber-500/10 text-amber-200"
                                : "border border-emerald-500/30 bg-emerald-500/10 text-emerald-200"
                              : "border border-zinc-700 bg-zinc-950/60 text-zinc-300"
                          }`}
                        >
                          {webhookStatusLabel(webhook)}
                        </span>
                        <div className="mt-2 text-xs text-zinc-500">
                          {testResult ? describeTestResult(testResult) : "Use Test to verify delivery."}
                        </div>
                      </td>
                      <td className="px-4 py-3">
                        <div className="flex flex-wrap gap-2">
                          <button
                            type="button"
                            onClick={() => void testMutation.mutateAsync(webhook)}
                            className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200"
                          >
                            Test
                          </button>
                          <button
                            type="button"
                            onClick={() => beginEdit(webhook)}
                            className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200"
                          >
                            Edit
                          </button>
                          <button
                            type="button"
                            onClick={() => setDeleteWebhook(webhook)}
                            className="rounded-lg border border-red-900 px-3 py-1.5 text-xs text-red-300"
                          >
                            Delete
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {webhooks.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="px-4 py-8 text-center text-sm text-zinc-400">
                      No webhooks configured yet.
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </section>
        </div>
      </main>

      <Dialog
        open={deleteWebhook !== null}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteWebhook(null);
          }
        }}
        titleId={deleteDialogTitleId}
        initialFocusRef={deleteCancelRef}
      >
        <div className="mx-auto w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-950 p-5 text-zinc-100 shadow-2xl">
          <h3 id={deleteDialogTitleId} className="text-xl font-semibold text-zinc-50">
            Delete webhook?
          </h3>
          <p className="mt-2 text-sm text-zinc-400">
            {deleteWebhook ? `This will remove ${deleteWebhook.url}.` : null}
          </p>
          <div className="mt-5 flex justify-end gap-2">
            <button
              ref={deleteCancelRef}
              type="button"
              onClick={() => setDeleteWebhook(null)}
              className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => {
                if (deleteWebhook) {
                  void deleteMutation.mutateAsync(deleteWebhook.id);
                }
                setDeleteWebhook(null);
              }}
              className="rounded-lg border border-red-500 px-3 py-2 text-sm text-red-300"
            >
              Delete
            </button>
          </div>
        </div>
      </Dialog>
    </div>
  );
}

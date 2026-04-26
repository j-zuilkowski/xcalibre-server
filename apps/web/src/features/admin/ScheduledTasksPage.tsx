/**
 * ScheduledTasksPage — admin cron task CRUD.
 *
 * Route: /admin/scheduled-tasks
 *
 * A scheduled task maps a cron expression to one of three task types:
 *   - classify_all     — runs the library-wide LLM classification job
 *   - semantic_index_all — rebuilds the vector/semantic index for all books
 *   - backup           — triggers the configured backup job
 *
 * Layout:
 *   - "Add task" form: name, task type selector, cron expression input.
 *     A human-readable cron description (`describeCronExpression` from
 *     admin-utils) is shown beneath the cron field.
 *   - Tasks table rendered via `ScheduledTaskRow` sub-component: name, type,
 *     cron expression, enabled toggle (inline PATCH on change), last-run and
 *     next-run timestamps, Delete button.
 *   - Delete confirmation Dialog.
 *   - Task type reference cards at the bottom of the page.
 *
 * The `disabled` prop on ScheduledTaskRow is set while any mutation is
 * pending to prevent concurrent conflicting updates.
 *
 * API calls:
 *   GET    /api/v1/admin/scheduled-tasks
 *   POST   /api/v1/admin/scheduled-tasks
 *   PATCH  /api/v1/admin/scheduled-tasks/:id
 *   DELETE /api/v1/admin/scheduled-tasks/:id
 */
import { useId, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type {
  ScheduledTask,
  ScheduledTaskCreateRequest,
  ScheduledTaskType,
} from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { Dialog } from "../../components/ui/Dialog";
import { describeCronExpression, formatDateTime } from "./admin-utils";

const TASK_TYPES: Array<{ value: ScheduledTaskType; label: string; description: string }> = [
  {
    value: "classify_all",
    label: "Classify all",
    description: "Queue the library-wide classification job.",
  },
  {
    value: "semantic_index_all",
    label: "Semantic index all",
    description: "Queue semantic indexing for every book.",
  },
  {
    value: "backup",
    label: "Backup",
    description: "Queue the backup job.",
  },
];

function taskTypeLabel(taskType: ScheduledTaskType): string {
  return TASK_TYPES.find((entry) => entry.value === taskType)?.label ?? taskType;
}

/**
 * ScheduledTaskRow renders a single row in the tasks table.
 *
 * @param task            - The scheduled task data to display.
 * @param onToggleEnabled - Called when the enabled checkbox changes.
 * @param onDelete        - Called when the Delete button is clicked.
 * @param disabled        - Disables interactive controls while a mutation is
 *                          pending to prevent concurrent conflicting updates.
 */
function ScheduledTaskRow({
  task,
  onToggleEnabled,
  onDelete,
  disabled,
}: {
  task: ScheduledTask;
  onToggleEnabled: (task: ScheduledTask, enabled: boolean) => void;
  onDelete: (task: ScheduledTask) => void;
  disabled: boolean;
}) {
  return (
    <tr className="border-t border-zinc-800">
      <td className="px-4 py-3 text-zinc-100">
        <div className="font-medium">{task.name}</div>
        <div className="text-xs text-zinc-500">{task.id.slice(0, 8)}</div>
      </td>
      <td className="px-4 py-3 text-zinc-300">
        <div>{taskTypeLabel(task.task_type)}</div>
        <div className="text-xs text-zinc-500">{task.task_type}</div>
      </td>
      <td className="px-4 py-3 font-mono text-zinc-200">{task.cron_expr}</td>
      <td className="px-4 py-3">
        <label className="inline-flex items-center gap-2 text-zinc-300">
          <input
            type="checkbox"
            checked={task.enabled}
            disabled={disabled}
            aria-label={`Enabled state for scheduled task ${task.name}`}
            onChange={(event) => onToggleEnabled(task, event.target.checked)}
          />
          {task.enabled ? "Enabled" : "Disabled"}
        </label>
      </td>
      <td className="px-4 py-3 text-zinc-300">{formatDateTime(task.last_run_at)}</td>
      <td className="px-4 py-3 text-zinc-300">{formatDateTime(task.next_run_at)}</td>
      <td className="px-4 py-3">
        <button
          type="button"
          onClick={() => onDelete(task)}
          disabled={disabled}
          className="rounded-lg border border-red-900 px-3 py-2 text-xs text-red-300 disabled:opacity-60"
        >
          Delete
        </button>
      </td>
    </tr>
  );
}

/**
 * ScheduledTasksPage renders the cron task management table with a creation
 * form, inline enabled toggle, and delete confirmation dialog.
 */
export function ScheduledTasksPage() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<ScheduledTaskCreateRequest>({
    name: "",
    task_type: "classify_all",
    cron_expr: "0 2 * * 0",
    enabled: true,
  });
  const [taskToDelete, setTaskToDelete] = useState<ScheduledTask | null>(null);
  const deleteDialogTitleId = useId();
  const deleteCancelRef = useRef<HTMLButtonElement | null>(null);

  const tasksQuery = useQuery({
    queryKey: ["admin-scheduled-tasks"],
    queryFn: () => apiClient.listScheduledTasks(),
  });

  const createMutation = useMutation({
    mutationFn: (payload: ScheduledTaskCreateRequest) => apiClient.createScheduledTask(payload),
    onSuccess: async () => {
      setForm({
        name: "",
        task_type: "classify_all",
        cron_expr: "0 2 * * 0",
        enabled: true,
      });
      await queryClient.invalidateQueries({ queryKey: ["admin-scheduled-tasks"] });
    },
  });

  const updateMutation = useMutation({
    mutationFn: (payload: { id: string; enabled?: boolean; cron_expr?: string }) =>
      apiClient.updateScheduledTask(payload.id, {
        enabled: payload.enabled,
        cron_expr: payload.cron_expr,
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-scheduled-tasks"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteScheduledTask(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-scheduled-tasks"] });
    },
  });

  const tasks = tasksQuery.data ?? [];

  return (
    <main className="mx-auto flex w-full max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">Scheduled tasks</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">Recurring automation</h2>
        <p className="mt-2 max-w-3xl text-sm text-zinc-400">
          Schedule recurring library jobs without wiring them up manually each time.
        </p>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <h3 className="text-lg font-semibold text-zinc-50">Add task</h3>
        <form
          className="mt-4 grid gap-3 lg:grid-cols-[1.2fr_1fr_1.2fr_auto]"
          onSubmit={(event) => {
            event.preventDefault();
            void createMutation.mutateAsync({
              name: form.name.trim(),
              task_type: form.task_type,
              cron_expr: form.cron_expr.trim(),
              enabled: form.enabled,
            });
          }}
        >
          <input
            value={form.name}
            onChange={(event) => setForm((previous) => ({ ...previous, name: event.target.value }))}
            placeholder="Task name"
            aria-label="Task name"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400"
          />
          <select
            value={form.task_type}
            onChange={(event) =>
              setForm((previous) => ({ ...previous, task_type: event.target.value as ScheduledTaskType }))
            }
            aria-label="Task type"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          >
            {TASK_TYPES.map((taskType) => (
              <option key={taskType.value} value={taskType.value}>
                {taskType.label}
              </option>
            ))}
          </select>
          <div>
            <input
              value={form.cron_expr}
              onChange={(event) => setForm((previous) => ({ ...previous, cron_expr: event.target.value }))}
              placeholder="0 2 * * 0"
              aria-label="Cron expression"
              className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm font-mono text-zinc-100 placeholder:text-zinc-400"
            />
            <p className="mt-2 text-xs text-zinc-500">{describeCronExpression(form.cron_expr)}</p>
          </div>
          <div className="flex items-center gap-3">
            <label className="inline-flex items-center gap-2 text-sm text-zinc-300">
              <input
                type="checkbox"
                checked={form.enabled}
                onChange={(event) =>
                  setForm((previous) => ({ ...previous, enabled: event.target.checked }))
                }
              />
              Enabled
            </label>
            <button
              type="submit"
              disabled={createMutation.isPending}
              className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-60"
            >
              {createMutation.isPending ? "Creating..." : "Add task"}
            </button>
          </div>
        </form>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th scope="col" className="px-4 py-3 font-medium">Name</th>
              <th scope="col" className="px-4 py-3 font-medium">Type</th>
              <th scope="col" className="px-4 py-3 font-medium">Cron expression</th>
              <th scope="col" className="px-4 py-3 font-medium">Enabled</th>
              <th scope="col" className="px-4 py-3 font-medium">Last run</th>
              <th scope="col" className="px-4 py-3 font-medium">Next run</th>
              <th scope="col" className="px-4 py-3 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {tasks.map((task) => (
              <ScheduledTaskRow
                key={task.id}
                task={task}
                disabled={updateMutation.isPending || deleteMutation.isPending}
                onToggleEnabled={(row, enabled) => {
                  void updateMutation.mutateAsync({ id: row.id, enabled });
                }}
                onDelete={(row) => {
                  setTaskToDelete(row);
                }}
              />
            ))}

            {!tasksQuery.isLoading && tasks.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-4 py-8 text-center text-sm text-zinc-400">
                  No scheduled tasks yet.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>

      <Dialog
        open={taskToDelete !== null}
        onOpenChange={(open) => {
          if (!open) {
            setTaskToDelete(null);
          }
        }}
        titleId={deleteDialogTitleId}
        initialFocusRef={deleteCancelRef}
      >
        <div className="mx-auto w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-950 p-5 text-zinc-100 shadow-2xl">
          <h3 id={deleteDialogTitleId} className="text-xl font-semibold text-zinc-50">
            Delete scheduled task?
          </h3>
          <p className="mt-2 text-sm text-zinc-400">
            {taskToDelete ? `This will remove "${taskToDelete.name}".` : null}
          </p>
          <div className="mt-5 flex justify-end gap-2">
            <button
              ref={deleteCancelRef}
              type="button"
              onClick={() => setTaskToDelete(null)}
              className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => {
                if (taskToDelete) {
                  void deleteMutation.mutateAsync(taskToDelete.id);
                }
                setTaskToDelete(null);
              }}
              className="rounded-lg border border-red-500 px-3 py-2 text-sm text-red-300"
            >
              Delete
            </button>
          </div>
        </div>
      </Dialog>

      <section className="grid gap-4 md:grid-cols-3">
        {TASK_TYPES.map((taskType) => (
          <div key={taskType.value} className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-4">
            <p className="text-sm font-semibold text-zinc-50">{taskType.label}</p>
            <p className="mt-2 text-sm text-zinc-400">{taskType.description}</p>
          </div>
        ))}
      </section>
    </main>
  );
}

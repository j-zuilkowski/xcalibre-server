import { useTranslation } from "react-i18next";
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { CustomColumnType } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";

type CreateColumnForm = {
  name: string;
  label: string;
  column_type: CustomColumnType;
  is_multiple: boolean;
};

const DEFAULT_FORM: CreateColumnForm = {
  name: "",
  label: "",
  column_type: "text",
  is_multiple: false,
};

export function CustomColumnsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [form, setForm] = useState<CreateColumnForm>(DEFAULT_FORM);

  const columnsQuery = useQuery({
    queryKey: ["custom-columns"],
    queryFn: () => apiClient.listCustomColumns(),
  });

  const createMutation = useMutation({
    mutationFn: (payload: CreateColumnForm) => apiClient.createCustomColumn(payload),
    onSuccess: async () => {
      setForm(DEFAULT_FORM);
      await queryClient.invalidateQueries({ queryKey: ["custom-columns"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteCustomColumn(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["custom-columns"] });
    },
  });

  const columns = columnsQuery.data ?? [];

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.metadata")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("admin.custom_columns")}</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <h3 className="text-lg font-semibold text-zinc-50">{t("admin.add_custom_column")}</h3>
        <form
          className="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-5"
          onSubmit={(event) => {
            event.preventDefault();
            void createMutation.mutateAsync(form);
          }}
        >
          <input
            value={form.name}
            onChange={(event) => setForm((previous) => ({ ...previous, name: event.target.value }))}
            placeholder={t("admin.display_name")}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          />
          <input
            value={form.label}
            onChange={(event) => setForm((previous) => ({ ...previous, label: event.target.value }))}
            placeholder={t("admin.internal_label")}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          />
          <select
            value={form.column_type}
            onChange={(event) =>
              setForm((previous) => ({
                ...previous,
                column_type: event.target.value as CustomColumnType,
              }))
            }
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          >
            <option value="text">text</option>
            <option value="integer">integer</option>
            <option value="float">float</option>
            <option value="bool">bool</option>
            <option value="datetime">datetime</option>
          </select>
          <label className="flex items-center gap-2 rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-200">
            <input
              type="checkbox"
              checked={form.is_multiple}
              onChange={(event) =>
                setForm((previous) => ({ ...previous, is_multiple: event.target.checked }))
              }
            />
            {t("admin.multi_value")}
          </label>
          <button
            type="submit"
            className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950"
          >
            {createMutation.isPending ? t("common.creating") : t("common.create")}
          </button>
        </form>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th className="px-4 py-3 font-medium">{t("common.name")}</th>
              <th className="px-4 py-3 font-medium">{t("common.label")}</th>
              <th className="px-4 py-3 font-medium">{t("common.type")}</th>
              <th className="px-4 py-3 font-medium">{t("common.multi")}</th>
              <th className="px-4 py-3 font-medium">{t("common.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {columns.map((column) => (
              <tr key={column.id} className="border-t border-zinc-800">
                <td className="px-4 py-3 text-zinc-100">{column.name}</td>
                <td className="px-4 py-3 text-zinc-300">{column.label}</td>
                <td className="px-4 py-3 text-zinc-200">{column.column_type}</td>
                <td className="px-4 py-3 text-zinc-200">{column.is_multiple ? t("common.yes") : t("common.no")}</td>
                <td className="px-4 py-3">
                  <button
                    type="button"
                    onClick={() => {
                      const ok = window.confirm(
                        t("admin.delete_custom_column_confirm"),
                      );
                      if (ok) {
                        void deleteMutation.mutateAsync(column.id);
                      }
                    }}
                    className="rounded-lg border border-red-500 px-3 py-2 text-xs font-semibold text-red-300"
                  >
                    {t("common.delete")}
                  </button>
                </td>
              </tr>
            ))}
            {columns.length === 0 ? (
              <tr>
                <td className="px-4 py-4 text-zinc-400" colSpan={5}>
                  {t("admin.no_custom_columns_defined")}
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>
    </div>
  );
}

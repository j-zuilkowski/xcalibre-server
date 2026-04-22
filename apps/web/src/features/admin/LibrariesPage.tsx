import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { apiClient } from "../../lib/api-client";

type LibraryFormState = {
  name: string;
  calibre_db_path: string;
};

export function LibrariesPage() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<LibraryFormState>({
    name: "",
    calibre_db_path: "",
  });

  const librariesQuery = useQuery({
    queryKey: ["libraries"],
    queryFn: () => apiClient.listLibraries(),
  });

  const createMutation = useMutation({
    mutationFn: (payload: LibraryFormState) => apiClient.createLibrary(payload),
    onSuccess: async () => {
      setForm({ name: "", calibre_db_path: "" });
      await queryClient.invalidateQueries({ queryKey: ["libraries"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteLibrary(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["libraries"] });
    },
  });

  const libraries = librariesQuery.data ?? [];

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">Libraries</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">Manage libraries</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <h3 className="text-lg font-semibold text-zinc-50">Add library</h3>
        <form
          className="mt-4 grid gap-3 md:grid-cols-[1fr_2fr_auto]"
          onSubmit={(event) => {
            event.preventDefault();
            void createMutation.mutateAsync(form);
          }}
        >
          <input
            value={form.name}
            onChange={(event) => setForm((previous) => ({ ...previous, name: event.target.value }))}
            placeholder="Library name"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          />
          <input
            value={form.calibre_db_path}
            onChange={(event) =>
              setForm((previous) => ({ ...previous, calibre_db_path: event.target.value }))
            }
            placeholder="/path/to/metadata.db"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          />
          <button
            type="submit"
            className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950"
          >
            {createMutation.isPending ? "Creating..." : "Create"}
          </button>
        </form>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th className="px-4 py-3 font-medium">Name</th>
              <th className="px-4 py-3 font-medium">Path</th>
              <th className="px-4 py-3 font-medium">Books</th>
              <th className="px-4 py-3 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {libraries.map((library) => (
              <tr key={library.id} className="border-t border-zinc-800">
                <td className="px-4 py-3 text-zinc-100">{library.name}</td>
                <td className="px-4 py-3 text-zinc-400">{library.calibre_db_path}</td>
                <td className="px-4 py-3 text-zinc-100">{library.book_count ?? 0}</td>
                <td className="px-4 py-3">
                  <button
                    type="button"
                    disabled={(library.book_count ?? 0) > 0}
                    title={(library.book_count ?? 0) > 0 ? "Delete books first" : "Delete library"}
                    onClick={() => void deleteMutation.mutateAsync(library.id)}
                    className="rounded-lg border border-red-500 px-3 py-2 text-xs font-semibold text-red-300 disabled:cursor-not-allowed disabled:border-zinc-700 disabled:text-zinc-500"
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}

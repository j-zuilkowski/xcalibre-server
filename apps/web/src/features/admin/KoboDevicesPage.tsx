import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { KoboDevice } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { formatDateTime } from "./admin-utils";

function formatSyncUrl(): string {
  const baseUrl = window.location.origin.replace(/\/$/, "");
  return `${baseUrl}/kobo/<api_token>/v1/`;
}

export function KoboDevicesPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  const devicesQuery = useQuery({
    queryKey: ["admin-kobo-devices"],
    queryFn: () => apiClient.listKoboDevices(),
  });

  const revokeMutation = useMutation({
    mutationFn: (id: string) => apiClient.revokeKoboDevice(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-kobo-devices"] });
    },
  });

  const devices = devicesQuery.data ?? [];

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.kobo")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("admin.kobo_devices")}</h2>
        <p className="mt-2 max-w-2xl text-sm text-zinc-400">
          {t("admin.kobo_description")}
        </p>
        <code className="mt-3 inline-block rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs text-zinc-200">
          {formatSyncUrl()}
        </code>
      </header>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th scope="col" className="px-4 py-3 font-medium">{t("admin.device")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("admin.user")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("admin.last_sync")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("admin.registered")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("common.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {devices.map((device: KoboDevice) => (
              <tr key={device.id} className="border-t border-zinc-800">
                <td className="px-4 py-3 text-zinc-100">
                  <div className="font-medium">{device.device_name}</div>
                  <div className="font-mono text-xs text-zinc-500">{device.device_id}</div>
                </td>
                <td className="px-4 py-3 text-zinc-300">
                  <div>{device.username}</div>
                  <div className="text-xs text-zinc-500">{device.email}</div>
                </td>
                <td className="px-4 py-3 text-zinc-300">
                  {device.last_sync_at ? formatDateTime(device.last_sync_at) : t("common.never")}
                </td>
                <td className="px-4 py-3 text-zinc-300">{formatDateTime(device.created_at)}</td>
                <td className="px-4 py-3">
                  <button
                    type="button"
                    onClick={() => void revokeMutation.mutateAsync(device.id)}
                    disabled={revokeMutation.isPending}
                    className="rounded-lg border border-red-900 px-3 py-2 text-xs text-red-300 disabled:opacity-60"
                  >
                    {t("common.revoke")}
                  </button>
                </td>
              </tr>
            ))}

            {!devicesQuery.isLoading && devices.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-8 text-center text-sm text-zinc-400">
                  {t("admin.no_kobo_devices_registered_yet")}
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>
    </div>
  );
}

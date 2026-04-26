import { afterEach } from "vitest";
import { QueryClient, type QueryClientConfig } from "@tanstack/react-query";

const trackedClients = new Set<QueryClient>();

export function makeTestQueryClient(config: QueryClientConfig = {}): QueryClient {
  const client = new QueryClient(config);
  trackedClients.add(client);
  return client;
}

afterEach(() => {
  for (const client of trackedClients) {
    client.clear();
  }
  trackedClients.clear();
});

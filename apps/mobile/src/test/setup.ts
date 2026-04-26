import { beforeAll, vi } from "vitest";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { DatabaseSync } = require("node:sqlite") as typeof import("node:sqlite");

vi.mock("expo-image", () => {
  return {
    Image: "Image",
  };
});

vi.mock("@expo/vector-icons", () => {
  return {
    Ionicons: "Ionicons",
    MaterialIcons: "MaterialIcons",
    Feather: "Feather",
    AntDesign: "AntDesign",
  };
});

vi.mock("expo-secure-store", () => {
  const store = new Map<string, string>();

  return {
    getItemAsync: vi.fn(async (key: string) => store.get(key) ?? null),
    setItemAsync: vi.fn(async (key: string, value: string) => {
      store.set(key, value);
    }),
    deleteItemAsync: vi.fn(async (key: string) => {
      store.delete(key);
    }),
  };
});

vi.mock("expo-file-system", () => {
  const resumable = {
    cancelAsync: vi.fn(async () => undefined),
    downloadAsync: vi.fn(async () => ({
      uri: "file:///documents/book-1.epub",
      status: 200,
      headers: {},
      mimeType: null,
    })),
    pauseAsync: vi.fn(async () => ({
      url: "http://example.test",
      fileUri: "file:///documents/book-1.epub",
      options: {},
      resumeData: null,
    })),
    resumeAsync: vi.fn(async () => ({
      uri: "file:///documents/book-1.epub",
      status: 200,
      headers: {},
      mimeType: null,
    })),
    savable: vi.fn(() => ({
      url: "http://example.test",
      fileUri: "file:///documents/book-1.epub",
      options: {},
      resumeData: null,
    })),
  };

  return {
    documentDirectory: "file:///documents/",
    EncodingType: {
      Base64: "base64",
      UTF8: "utf8",
    },
    downloadAsync: vi.fn(async (_uri: string, fileUri: string) => ({
      uri: fileUri,
      status: 200,
      headers: {},
      mimeType: null,
    })),
    writeAsStringAsync: vi.fn(async () => undefined),
    deleteAsync: vi.fn(async () => undefined),
    makeDirectoryAsync: vi.fn(async () => undefined),
    getInfoAsync: vi.fn(async (fileUri: string) => ({
      exists: false,
      isDirectory: false,
      uri: fileUri,
    })),
    getFreeDiskStorageAsync: vi.fn(async () => 10 * 1024 * 1024 * 1024),
    createDownloadResumable: vi.fn(() => resumable),
  };
});

vi.mock("@react-native-community/netinfo", () => {
  const state = {
    type: "wifi",
    isConnected: true,
    isInternetReachable: true,
    isWifiEnabled: true,
    details: null,
  };

  return {
    fetch: vi.fn(async () => state),
    refresh: vi.fn(async () => state),
    addEventListener: vi.fn(() => () => undefined),
    useNetInfo: vi.fn(() => state),
  };
});

vi.mock("react-native-localize", () => {
  return {
    getLocales: () => [{ languageCode: "en", languageTag: "en-US" }],
  };
});

vi.mock("expo-sqlite", () => {
  const openDatabases = new Set<{ closeAsync: () => Promise<void>; closeSync: () => void }>();

  type Statement = {
    run(...params: Array<unknown>): { changes: number; lastInsertRowid: bigint };
    get(...params: Array<unknown>): Record<string, unknown> | undefined;
    all(...params: Array<unknown>): Array<Record<string, unknown>>;
  };

  function runStatement(
    statement: Statement,
    params?: unknown[] | Record<string, unknown>,
  ) {
    if (Array.isArray(params)) {
      return statement.run(...params);
    }

    if (params === undefined) {
      return statement.run();
    }

    return statement.run(params);
  }

  function getStatement(
    statement: Statement,
    params?: unknown[] | Record<string, unknown>,
  ) {
    if (Array.isArray(params)) {
      return statement.get(...params);
    }

    if (params === undefined) {
      return statement.get();
    }

    return statement.get(params);
  }

  function allStatement(
    statement: Statement,
    params?: unknown[] | Record<string, unknown>,
  ) {
    if (Array.isArray(params)) {
      return statement.all(...params);
    }

    if (params === undefined) {
      return statement.all();
    }

    return statement.all(params);
  }

  function wrapStatement(statement: Statement) {
    return {
      executeAsync: async (params?: unknown[] | Record<string, unknown>) => ({
        ...(() => {
          const result = runStatement(statement, params);
          return {
            changes: result.changes,
            lastInsertRowId: Number(result.lastInsertRowid),
          };
        })(),
      }),
      executeSync: (params?: unknown[] | Record<string, unknown>) => ({
        ...(() => {
          const result = runStatement(statement, params);
          return {
            changes: result.changes,
            lastInsertRowId: Number(result.lastInsertRowid),
          };
        })(),
      }),
      finalizeAsync: async () => undefined,
      finalizeSync: () => undefined,
    };
  }

  function createDatabase() {
    const db = new DatabaseSync(":memory:");

    const database = {
      databasePath: ":memory:",
      options: {},
      execAsync: async (source: string) => {
        db.exec(source);
      },
      runAsync: async (source: string, params?: unknown[] | Record<string, unknown>) => {
        const statement = db.prepare(source);
        const result = runStatement(statement as unknown as Statement, params);
        return {
          changes: result.changes,
          lastInsertRowId: Number(result.lastInsertRowid),
        };
      },
      getFirstAsync: async <T,>(
        source: string,
        params?: unknown[] | Record<string, unknown>,
      ): Promise<T | null> => {
        const statement = db.prepare(source);
        const row = getStatement(statement as unknown as Statement, params) as T | undefined;
        return row ?? null;
      },
      getAllAsync: async <T,>(
        source: string,
        params?: unknown[] | Record<string, unknown>,
      ): Promise<T[]> => {
        const statement = db.prepare(source);
        return allStatement(statement as unknown as Statement, params) as T[];
      },
      withTransactionAsync: async (task: () => Promise<void>) => {
        db.exec("BEGIN");
        try {
          await task();
          db.exec("COMMIT");
        } catch (error) {
          db.exec("ROLLBACK");
          throw error;
        }
      },
      withExclusiveTransactionAsync: async (task: () => Promise<void>) => {
        db.exec("BEGIN");
        try {
          await task();
          db.exec("COMMIT");
        } catch (error) {
          db.exec("ROLLBACK");
          throw error;
        }
      },
      prepareAsync: async (source: string) => wrapStatement(db.prepare(source) as unknown as Statement),
      prepareSync: (source: string) => wrapStatement(db.prepare(source) as unknown as Statement),
      closeAsync: async () => {
        db.close();
      },
      closeSync: () => {
        db.close();
      },
    };

    openDatabases.add(database);
    return database;
  }

  afterAll(() => {
    for (const database of openDatabases) {
      database.closeSync();
    }
    openDatabases.clear();
  });

  return {
    openDatabaseAsync: async () => createDatabase(),
    openDatabaseSync: () => createDatabase(),
  };
});

beforeAll(async () => {
  const { initializeI18n } = await import("../i18n");
  await initializeI18n();
});

import { drizzle as drizzleNodePg } from "drizzle-orm/node-postgres";
import { drizzle as drizzlePglite } from "drizzle-orm/pglite";
import { PGlite } from "@electric-sql/pglite";
import { Pool } from "pg";
import { mkdirSync } from "node:fs";
import path from "node:path";
import { EMBEDDED_SCHEMA } from "@/db/embedded-schema";
import { embeddedDirectory } from "@/lib/paths";

/**
 * Ghost Music talks to PostgreSQL when DATABASE_URL is configured (the normal
 * server deployment). Packaged builds — the Windows desktop app and the
 * standalone mobile shell — ship without a database server, so they open an
 * embedded PGlite database file inside the app's data directory instead.
 * PGlite is real PostgreSQL compiled to WebAssembly, so the Drizzle schema and
 * every query stay identical between the two modes.
 */
const databaseUrl = process.env.DATABASE_URL;

type Database = ReturnType<typeof drizzleNodePg>;

const globalForDb = globalThis as typeof globalThis & {
  __arenaNextJsPostgresqlPool?: Pool;
  __ghostEmbeddedDb?: { db: Database; ready: Promise<void> };
};

export const usingEmbeddedDatabase = !databaseUrl;

function createEmbedded(): { db: Database; ready: Promise<void> } {
  const directory = embeddedDirectory();
  mkdirSync(directory, { recursive: true });
  const client = new PGlite(directory);
  // PGlite serialises work through a single queue, so this DDL always runs
  // before the first query the app issues.
  const ready = client.exec(EMBEDDED_SCHEMA).then(() => {});
  return { db: drizzlePglite(client) as unknown as Database, ready };
}

function create(): { db: Database; ready: Promise<void> } {
  if (!databaseUrl) return createEmbedded();
  const pool = globalForDb.__arenaNextJsPostgresqlPool ?? new Pool({ connectionString: databaseUrl });
  if (process.env.NODE_ENV !== "production") globalForDb.__arenaNextJsPostgresqlPool = pool;
  return { db: drizzleNodePg(pool), ready: Promise.resolve() };
}

const active = globalForDb.__ghostEmbeddedDb ?? create();
if (usingEmbeddedDatabase) globalForDb.__ghostEmbeddedDb = active;

export const db = active.db;
export const databaseReady = active.ready;

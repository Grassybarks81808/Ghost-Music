import { db, databaseReady, usingEmbeddedDatabase } from "@/db";
import { sql } from "drizzle-orm";

export const dynamic = "force-dynamic";

export async function GET() {
  try {
    await databaseReady;
    await db.execute(sql`select 1`);
    return Response.json({ ok: true, database: usingEmbeddedDatabase ? "embedded" : "postgres" });
  } catch {
    return Response.json({ ok: false }, { status: 500 });
  }
}

import path from "node:path";
import { mkdirSync } from "node:fs";

/**
 * Writable location for downloads and the embedded library. Server deployments
 * default to the project directory; packaged apps point GHOST_DATA_DIR at the
 * user's app-data folder because their install directory is read-only.
 */
export function dataDirectory() {
  return process.env.GHOST_DATA_DIR || process.cwd();
}

/** File-based library used when no DATABASE_URL is configured. */
export function embeddedDirectory() {
  return path.join(dataDirectory(), ".ghost-library");
}

export function downloadsDirectory() {
  const directory = path.join(dataDirectory(), ".ghost-downloads");
  mkdirSync(directory, { recursive: true });
  return directory;
}

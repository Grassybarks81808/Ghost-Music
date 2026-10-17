/**
 * Downloads inside a WebView cannot follow a `blob:` anchor, so the Android
 * shell exposes a small bridge that writes the bytes into Music/Ghost Music.
 * Everywhere else this is a no-op and the normal browser download runs.
 */
type GhostNativeBridge = {
  saveFile(name: string, base64Chunk: string, append: boolean): boolean;
  finishFile(): boolean;
};

const CHUNK = 384 * 1024;

function bridge(): GhostNativeBridge | undefined {
  return (globalThis as typeof globalThis & { GhostMusic?: GhostNativeBridge }).GhostMusic;
}

function toBase64(bytes: Uint8Array) {
  let binary = "";
  const step = 0x8000;
  for (let index = 0; index < bytes.length; index += step) {
    binary += String.fromCharCode.apply(null, Array.from(bytes.subarray(index, index + step)));
  }
  return btoa(binary);
}

export async function saveToDevice(parts: BlobPart[], filename: string): Promise<boolean> {
  const native = bridge();
  if (!native?.saveFile) return false;
  let written = 0;
  try {
    for (const part of parts) {
      const view = part instanceof Uint8Array ? part : new Uint8Array(part as ArrayBuffer);
      for (let offset = 0; offset < view.byteLength; offset += CHUNK) {
        const slice = view.subarray(offset, Math.min(offset + CHUNK, view.byteLength));
        if (!native.saveFile(filename, toBase64(slice), written > 0)) return false;
        written += slice.byteLength;
      }
    }
    if (!written) return false;
    return native.finishFile();
  } catch {
    return false;
  }
}

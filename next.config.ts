import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Packaged builds (desktop/mobile shells) run the standalone server, so the
  // media tools and the embedded WASM database must ship with it.
  output: process.env.GHOST_STANDALONE === "1" ? "standalone" : undefined,
  serverExternalPackages: ["ffmpeg-static", "youtube-dl-exec", "@electric-sql/pglite"],
  outputFileTracingIncludes: {
    "/api/**/*": [
      "./node_modules/youtube-dl-exec/bin/**/*",
      "./node_modules/ffmpeg-static/ffmpeg*",
      "./node_modules/@electric-sql/pglite/**/*",
      "./public/audio/**/*",
    ],
  },
};

export default nextConfig;

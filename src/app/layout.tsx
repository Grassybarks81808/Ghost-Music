import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import "@fontsource-variable/inter";
import "./globals.css";

export const metadata: Metadata = {
  title: "Ghost Music — Your music. Your other side.",
  description: "A little discovery. A little obsession. Stream, collect, and take your favorite sounds offline with Ghost Music.",
  applicationName: "Ghost Music",
  manifest: "/manifest.webmanifest",
  icons: { apple: "/icons/apple-touch-icon.png" },
  appleWebApp: { capable: true, statusBarStyle: "black-translucent", title: "Ghost Music" },
};
export const viewport: Viewport = { width: "device-width", initialScale: 1, themeColor: "#101012" };

export default function RootLayout({ children }: { children: ReactNode }) {
  return <html lang="en"><body>{children}</body></html>;
}

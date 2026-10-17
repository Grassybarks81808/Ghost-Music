import type { CSSProperties } from "react";

export function Youtube({ size = 20, className = "" }: { size?: number; className?: string }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true"><rect x="2" y="5" width="20" height="14" rx="4.5" stroke="currentColor" strokeWidth="1.6"/><path d="m10 9 5 3-5 3V9Z" fill="currentColor"/></svg>;
}

export function GhostIcon({ size = 24, className = "", headphones = false, style }: { size?: number; className?: string; headphones?: boolean; style?: CSSProperties }) {
  return <svg width={size} height={size} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" className={className} style={style}>
    <path d="M11 32C11 17.8 19.3 9 32 9s21 8.8 21 23v20c0 3.7-3.7 5.5-6.7 3.4l-5.1-3.5-5.4 4.2a6.3 6.3 0 0 1-7.6 0l-5.4-4.2-5.1 3.5C14.7 57.5 11 55.7 11 52V32Z" fill="currentColor"/>
    <rect x="23" y="28" width="5.5" height="9" rx="2.75" fill="#141416"/>
    <rect x="36" y="28" width="5.5" height="9" rx="2.75" fill="#141416"/>
    {headphones && <g className="ghost-headphones"><path d="M7 31v-2C7 15.2 18.2 4 32 4s25 11.2 25 25v3" stroke="#242428" strokeWidth="7" strokeLinecap="round"/><path d="M7 29v12M57 29v12" stroke="#ff665e" strokeWidth="10" strokeLinecap="round"/><path d="M9 31v8M55 31v8" stroke="#34343a" strokeWidth="5" strokeLinecap="round"/></g>}
  </svg>;
}

import { Play, ArrowUpRight } from "lucide-react";
import { GhostIcon } from "./ghost-icon";
import type { Playlist } from "@/lib/catalog";

const coverWords: Record<string, string> = { "mix-after-hours": "AFTER\nHOURS", "mix-clouds": "head in\nthe clouds", "mix-neon": "NEON\nNOSTALGIA", "mix-indie": "indie state\nof mind", "mix-focus": "DEEP\nFOCUS", "mix-lofi": "lowkey\n& lo-fi" };

export function MixCard({ mix, index = 0, onOpen, onPlay }: { mix: Playlist; index?: number; onOpen: () => void; onPlay: () => void }) {
  return <article className="mix-card" style={{ animationDelay: `${index * 45}ms` }}>
    <div className={`mix-art ${mix.id}`}>
      <button className="mix-art-link" onClick={onOpen} aria-label={`Open ${mix.name}`}><img src={mix.artwork} alt={`${mix.name} playlist artwork`} loading="eager" /><span className="cover-shade" /><span className="cover-mark"><GhostIcon size={15} /> GHOST MIX</span><span className="cover-title">{coverWords[mix.id] || mix.name}</span><span className="cover-corner"><ArrowUpRight size={14} /></span></button>
      <button className="mix-play" onClick={onPlay} aria-label={`Play ${mix.name}`}><Play size={19} fill="currentColor" /></button>
    </div>
    <button className="mix-title" onClick={onOpen}>{mix.name}</button><p>{mix.description || `${mix.trackIds.length} tracks · Your own little universe`}</p>
  </article>;
}

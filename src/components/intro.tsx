"use client";

import { useEffect } from "react";
import { ArrowRight, Music2, Music } from "lucide-react";
import { GhostIcon } from "./ghost-icon";

export function GhostIntro({ onDone }: { onDone: () => void }) {
  useEffect(() => { const timer = setTimeout(onDone, 10000); return () => clearTimeout(timer); }, [onDone]);
  return <div className="ghost-intro" role="dialog" aria-modal="true" aria-label="Welcome to Ghost Music">
    <div className="intro-brand"><GhostIcon size={29} /><span>GHOST MUSIC</span></div>
    <div className="intro-center">
      <div className="intro-aura" />
      <div className="intro-mascot"><GhostIcon size={160} headphones /><Music2 className="intro-note note-one" size={26} /><Music className="intro-note note-two" size={20} /><Music2 className="intro-note note-three" size={34} /></div>
      <div className="intro-copy"><span className="eyebrow">YOUR OTHER SIDE IS CALLING</span><h1>A little soul.<br />A lot of good spirits.</h1><p>Welcome to Ghost Music.</p></div>
    </div>
    <button className="intro-skip" onClick={onDone} autoFocus>Skip the ritual <ArrowRight size={16} /></button>
    <span className="intro-footnote">MADE FOR THE MUSIC. NOT THE NOISE.</span>
  </div>;
}

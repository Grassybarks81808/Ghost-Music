"use client";

import { Heart, Shuffle, SkipBack, SkipForward, Play, Pause, Repeat, Repeat1, ListMusic, Download, Volume2, VolumeX, Maximize2, LoaderCircle } from "lucide-react";
import { formatTime } from "@/lib/catalog";
import type { usePlayer } from "@/hooks/use-player";

type Props = { player: ReturnType<typeof usePlayer>; liked: boolean; onLike: () => void; onDownload: () => void; onQueue: () => void; onExpand: () => void; queueOpen: boolean; };
export function PlayerBar({ player: p, liked, onLike, onDownload, onQueue, onExpand, queueOpen }: Props) {
  const percentage = p.duration > 0 ? (p.time / p.duration) * 100 : 0;
  return <footer className={`player-bar ${p.playing ? "is-playing" : ""}`} aria-label="Music player">
    <div className="now-playing">
      <button className="playing-art" onClick={onExpand} aria-label="Open now playing"><img src={p.current.artwork} alt={`${p.current.album} cover`} /><span><Maximize2 size={18} /></span></button>
      <div className="playing-copy"><button onClick={onExpand}>{p.current.title}</button><span>{p.buffering ? "Finding your frequency…" : p.current.artist}</span></div>
      <button className={`icon-button player-like ${liked ? "is-liked" : ""}`} onClick={onLike} aria-label={liked ? "Unlike current song" : "Like current song"} aria-pressed={liked}><Heart size={18} fill={liked ? "currentColor" : "none"} /></button>
    </div>
    <div className="player-center">
      <div className="transport-controls">
        <button className={`icon-button small-control ${p.shuffle ? "is-active" : ""}`} onClick={() => p.setShuffle(!p.shuffle)} aria-label="Shuffle" aria-pressed={p.shuffle}><Shuffle size={16} /></button>
        <button className="icon-button" onClick={() => p.next(-1)} aria-label="Previous track"><SkipBack size={19} fill="currentColor" /></button>
        <button className="main-play" onClick={p.toggle} aria-label={p.playing ? "Pause" : "Play"}>{p.buffering ? <LoaderCircle className="spin" size={20} /> : p.playing ? <Pause size={19} fill="currentColor" /> : <Play size={19} fill="currentColor" className="play-offset" />}</button>
        <button className="icon-button" onClick={() => p.next(1)} aria-label="Next track"><SkipForward size={19} fill="currentColor" /></button>
        <button className={`icon-button small-control ${p.repeat !== "off" ? "is-active" : ""}`} onClick={p.cycleRepeat} aria-label={`Repeat: ${p.repeat}`} title={`Repeat: ${p.repeat}`} aria-pressed={p.repeat !== "off"}>{p.repeat === "one" ? <Repeat1 size={17} /> : <Repeat size={16} />}</button>
      </div>
      <div className="timeline"><span>{formatTime(p.time)}</span><input className="ghost-range" type="range" min="0" max={p.duration || 1} step="0.1" value={Math.min(p.time, p.duration || 1)} onChange={event => p.seek(Number(event.target.value))} aria-label="Seek through track" aria-valuetext={`${formatTime(p.time)} of ${formatTime(p.duration)}`} style={{ background: `linear-gradient(to right, var(--accent) ${percentage}%, #38353a ${percentage}%)` }} /><span>{formatTime(p.duration)}</span></div>
    </div>
    <div className="player-extras">
      <button className={`icon-button ${queueOpen ? "is-active" : ""}`} onClick={onQueue} aria-label="Open play queue" title="Your queue"><ListMusic size={19} /></button>
      <button className="icon-button" onClick={onDownload} aria-label="Download current track" title="Download current track"><Download size={18} /></button>
      <span className="control-divider" />
      <button className="icon-button" onClick={() => p.changeVolume(p.volume ? 0 : .7)} aria-label={p.volume ? "Mute" : "Unmute"}>{p.volume ? <Volume2 size={19} /> : <VolumeX size={19} />}</button>
      <input className="volume-range" type="range" min="0" max="1" step="0.01" value={p.volume} onChange={event => p.changeVolume(Number(event.target.value))} aria-label="Volume" style={{ background: `linear-gradient(to right, #c5c1c4 ${p.volume * 100}%, #38353a ${p.volume * 100}%)` }} />
      <button className="icon-button expand-control" onClick={onExpand} aria-label="Expand player" title="Now playing"><Maximize2 size={17} /></button>
    </div>
  </footer>;
}

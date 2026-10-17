"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { catalog, type Track } from "@/lib/catalog";

export function usePlayer(onError: (message: string) => void, onTrack: (track: Track) => void) {
  const [current, setCurrent] = useState<Track>(catalog[0]);
  const [queue, setQueue] = useState<Track[]>(catalog);
  const [playing, setPlaying] = useState(false);
  const [buffering, setBuffering] = useState(false);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(catalog[0].duration);
  const [volume, setVolume] = useState(.7);
  const [shuffle, setShuffle] = useState(false);
  const [repeat, setRepeat] = useState<"off" | "all" | "one">("off");
  const audio = useRef<HTMLAudioElement | null>(null);
  const context = useRef<AudioContext | null>(null);
  const analyser = useRef<AnalyserNode | null>(null);
  const refs = useRef({ current, queue, shuffle, repeat, onError, onTrack });
  refs.current = { current, queue, shuffle, repeat, onError, onTrack };
  const nextRef = useRef<(direction?: number, automatic?: boolean) => void>(() => {});

  useEffect(() => {
    const element = new Audio();
    element.preload = "metadata";
    element.crossOrigin = "anonymous";
    element.volume = .7;
    audio.current = element;
    element.ontimeupdate = () => setTime(element.currentTime);
    element.ondurationchange = () => { if (Number.isFinite(element.duration)) setDuration(element.duration); };
    element.onplay = () => setPlaying(true);
    element.onpause = () => setPlaying(false);
    element.onwaiting = () => setBuffering(true);
    element.onplaying = () => setBuffering(false);
    element.onended = () => nextRef.current(1, true);
    element.onerror = () => { setBuffering(false); setPlaying(false); refs.current.onError("This audio source is unavailable or restricted. Try another track, or play a Ghost Sessions original."); };
    let frame = 0;
    const values = new Uint8Array(128);
    const animate = () => {
      let energy = 0;
      if (analyser.current && !element.paused) {
        analyser.current.getByteFrequencyData(values);
        for (let i = 1; i < 24; i++) energy += values[i];
        energy /= 23 * 255;
      }
      document.documentElement.style.setProperty("--beat", energy.toFixed(3));
      document.documentElement.style.setProperty("--beat-hue", String(Math.round(350 + energy * 100)));
      frame = requestAnimationFrame(animate);
    };
    animate();
    return () => { cancelAnimationFrame(frame); element.pause(); element.src = ""; audio.current = null; void context.current?.close(); context.current = null; analyser.current = null; };
  }, []);

  const connect = useCallback(async () => {
    if (!audio.current) return;
    try {
      if (!context.current) {
        const ctx = new AudioContext();
        const source = ctx.createMediaElementSource(audio.current);
        const node = ctx.createAnalyser(); node.fftSize = 256; node.smoothingTimeConstant = .75;
        source.connect(node); node.connect(ctx.destination);
        context.current = ctx; analyser.current = node;
      }
      if (context.current.state === "suspended") await context.current.resume();
    } catch { /* Native playback is still available without visual analysis. */ }
  }, []);

  const playTrack = useCallback((track: Track, list?: Track[]) => {
    const element = audio.current;
    if (!element) return;
    if (list?.length) setQueue(list);
    setCurrent(track); setTime(0); setDuration(track.duration); setBuffering(true);
    element.src = track.source === "ghost" && track.audioPath ? track.audioPath : `/api/stream/${encodeURIComponent(track.id)}`;
    void connect();
    void element.play().catch(error => { if (error.name !== "AbortError") { setBuffering(false); setPlaying(false); refs.current.onError("Playback could not start. The source may be restricted; try another track."); } });
    refs.current.onTrack(track);
  }, [connect]);

  const toggle = useCallback(() => {
    const element = audio.current;
    if (!element) return;
    if (!element.getAttribute("src")) { playTrack(refs.current.current); return; }
    if (element.paused) { void connect(); void element.play().catch(() => refs.current.onError("Tap play to resume your music.")); }
    else element.pause();
  }, [connect, playTrack]);

  const next = useCallback((direction = 1, automatic = false) => {
    const state = refs.current;
    if (direction === -1 && (audio.current?.currentTime || 0) > 3) { if (audio.current) audio.current.currentTime = 0; return; }
    if (automatic && state.repeat === "one") { playTrack(state.current); return; }
    const index = state.queue.findIndex(t => t.id === state.current.id);
    let nextIndex = index + direction;
    if (state.shuffle && state.queue.length > 1) { do { nextIndex = Math.floor(Math.random() * state.queue.length); } while (nextIndex === index); }
    if (automatic && nextIndex >= state.queue.length && state.repeat === "off") { setPlaying(false); return; }
    const target = state.queue[(nextIndex + state.queue.length) % state.queue.length];
    if (target) playTrack(target);
  }, [playTrack]);
  nextRef.current = next;

  const seek = useCallback((value: number) => {
    if (audio.current && Number.isFinite(audio.current.duration)) { audio.current.currentTime = Math.max(0, Math.min(value, audio.current.duration)); setTime(audio.current.currentTime); }
  }, []);
  const changeVolume = useCallback((value: number) => { setVolume(value); if (audio.current) audio.current.volume = value; }, []);
  const cycleRepeat = useCallback(() => setRepeat(value => value === "off" ? "all" : value === "all" ? "one" : "off"), []);
  return { current, queue, setQueue, playing, buffering, time, duration, volume, shuffle, repeat, playTrack, toggle, next, seek, changeVolume, setShuffle, cycleRepeat };
}

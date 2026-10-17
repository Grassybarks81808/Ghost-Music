"use client";

import { useState, useEffect, useRef, useCallback, type FormEvent } from "react";
import { Home, Compass, Library, Heart, Download, History, Plus, Search, ChevronLeft, ChevronRight, ChevronDown, ArrowRight, ArrowUpRight, Play, Pause, Headphones, SlidersHorizontal, X, Check, CheckCircle2, AlertCircle, LoaderCircle, Music2, MoreHorizontal, Shuffle, Link2, Copy, Trash2, Clock3, Sparkles, Menu, WifiOff, Radio, FolderPlus, ShieldCheck, HardDrive, Keyboard, RotateCcw } from "lucide-react";
import { GhostIcon, Youtube } from "./ghost-icon";
import { GhostIntro } from "./intro";
import { PlayerBar } from "./player-bar";
import { MixCard } from "./mix-card";
import { Modal } from "./modal";
import { usePlayer } from "@/hooks/use-player";
import { catalog, mixes, initialPlaylists, formatTime, type Track, type Playlist, type DownloadJob } from "@/lib/catalog";
import { saveToDevice } from "@/lib/device";

type LibraryData = { tracks: Track[]; playlists: Playlist[]; liked: string[]; history: string[]; downloaded: string[] };
type Transfer = { track: Track; progress: number; bytes: number; total: number; status: string; error?: string; metadata?: Record<string, string | number> | null };
type ModalType = "import" | "create" | "settings" | "track" | "now-playing" | "share" | "permission" | "delete" | null;
const moodNames = ["For you", "Chill", "Energy", "Focus", "Late night", "Feel good"];
const moodMixes: Record<string, string[]> = { Chill: ["mix-clouds", "mix-lofi", "mix-focus"], Energy: ["mix-neon", "mix-after-hours", "mix-indie"], Focus: ["mix-focus", "mix-lofi", "mix-clouds"], "Late night": ["mix-after-hours", "mix-neon", "mix-lofi"], "Feel good": ["mix-indie", "mix-clouds", "mix-lofi"] };
const sizeLabel = (size: number) => `${(size / 1024 / 1024).toFixed(1)} MB`;
const errorMessage = (error: unknown) => error instanceof Error ? error.message : "Something went wrong. Give it another try.";

function TrackRow({ track, index, current, playing, liked, full = false, onPlay, onLike, onDownload, onDetails }: { track: Track; index: number; current: boolean; playing: boolean; liked: boolean; full?: boolean; onPlay: () => void; onLike: () => void; onDownload: () => void; onDetails: () => void }) {
  return <div className={`track-row ${full ? "track-row-full" : ""} ${current && playing ? "track-playing" : ""}`}>
    {full && <span className="track-number">{current && playing ? <span className="equalizer"><i /><i /><i /></span> : String(index + 1).padStart(2, "0")}</span>}
    <button className="track-main" onClick={onPlay} aria-label={`${current && playing ? "Pause" : "Play"} ${track.title}`}>
      <span className="track-image"><img src={track.artwork} alt="" loading="lazy" /><span>{current && playing ? <Pause size={18} fill="currentColor" /> : <Play size={18} fill="currentColor" />}</span></span>
      <span className="track-copy"><strong>{track.title}</strong><span>{track.artist}{!full && <><span className="tiny-dot"> · </span>{track.album}</>}</span></span>
    </button>
    {full && <span className="track-album">{track.album}</span>}
    <span className="track-duration">{track.duration ? formatTime(track.duration) : "—"}</span>
    <button className={`icon-button row-like ${liked ? "is-liked" : ""}`} aria-label={`${liked ? "Unlike" : "Like"} ${track.title}`} aria-pressed={liked} onClick={onLike}><Heart size={16} fill={liked ? "currentColor" : "none"} /></button>
    {full && <button className="icon-button row-download" onClick={onDownload} aria-label={`Download ${track.title}`}><Download size={17} /></button>}
    <button className="icon-button" onClick={onDetails} aria-label={`More options for ${track.title}`}><MoreHorizontal size={19} /></button>
  </div>;
}

export default function GhostMusicApp() {
  const [tracks, setTracks] = useState<Track[]>(catalog);
  const [playlists, setPlaylists] = useState<Playlist[]>(initialPlaylists.map((p, i) => ({ ...p, id: `initial-${i}`, description: "Your own little universe." })));
  const [liked, setLiked] = useState<string[]>([]);
  const [history, setHistory] = useState<string[]>([]);
  const [downloaded, setDownloaded] = useState<string[]>([]);
  const [jobs, setJobs] = useState<DownloadJob[]>([]);
  const [view, setView] = useState("Home");
  const [navigation, setNavigation] = useState({ items: ["Home"], index: 0 });
  const [mood, setMood] = useState("For you");
  const [libraryTab, setLibraryTab] = useState("Playlists");
  const [search, setSearch] = useState("");
  const [searchTerm, setSearchTerm] = useState("");
  const [searchResults, setSearchResults] = useState<Track[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState("");
  const [modal, setModal] = useState<ModalType>(null);
  const [detailTrack, setDetailTrack] = useState<Track>(catalog[0]);
  const [queueOpen, setQueueOpen] = useState(false);
  const [downloadPanel, setDownloadPanel] = useState(false);
  const [transfer, setTransfer] = useState<Transfer | null>(null);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [ghostMode, setGhostMode] = useState(true);
  const [displayName, setDisplayName] = useState("Ghost listener");
  const [intro, setIntro] = useState(false);
  const [offline, setOffline] = useState(false);
  const [formBusy, setFormBusy] = useState(false);
  const [formError, setFormError] = useState("");
  const [importUrl, setImportUrl] = useState("");
  const [playlistName, setPlaylistName] = useState("");
  const [playlistDescription, setPlaylistDescription] = useState("");
  const [addTrackId, setAddTrackId] = useState<string | null>(null);
  const [shareTarget, setShareTarget] = useState({ name: "Ghost Music", url: "" });
  const [sharedPlaylist, setSharedPlaylist] = useState<Playlist | null>(null);
  const [toast, setToast] = useState<{ message: string; kind: string } | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const listener = useRef("");
  const searchInput = useRef<HTMLInputElement>(null);
  const scrollArea = useRef<HTMLDivElement>(null);
  const downloadLock = useRef(false);
  const searchAbort = useRef<AbortController | null>(null);

  const notify = useCallback((message: string, kind = "success") => { setToast({ message, kind }); if (toastTimer.current) clearTimeout(toastTimer.current); toastTimer.current = setTimeout(() => setToast(null), 5000); }, []);
  const getListener = useCallback(() => {
    if (!listener.current) { const saved = localStorage.getItem("ghost-listener"); listener.current = saved && /^[a-f0-9-]{36}$/i.test(saved) ? saved : crypto.randomUUID(); localStorage.setItem("ghost-listener", listener.current); }
    return listener.current;
  }, []);
  const api = useCallback(async <T,>(url: string, options: RequestInit = {}): Promise<T> => {
    const response = await fetch(url, { ...options, headers: { "Content-Type": "application/json", "x-ghost-listener": getListener(), ...options.headers } });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || "Could not connect to Ghost Music. Please try again.");
    return data as T;
  }, [getListener]);

  const loadLibrary = useCallback(async () => {
    const data = await api<LibraryData>("/api/library");
    setTracks(data.tracks); setPlaylists(data.playlists); setLiked(data.liked); setHistory(data.history); setDownloaded(data.downloaded);
    localStorage.setItem("ghost-library", JSON.stringify(data));
    return data;
  }, [api]);

  const recordPlay = useCallback((track: Track) => {
    setHistory(items => [track.id, ...items.filter(id => id !== track.id)]);
    void api("/api/library", { method: "POST", body: JSON.stringify({ action: "history", trackId: track.id }) }).catch(() => {});
  }, [api]);
  const player = usePlayer(message => notify(message, "error"), recordPlay);
  const playerRef = useRef(player); playerRef.current = player;

  const navigate = useCallback((target: string) => {
    setView(target); setNavigation(old => old.items[old.index] === target ? old : { items: [...old.items.slice(0, old.index + 1), target], index: old.index + 1 });
    setMobileOpen(false); scrollArea.current?.scrollTo({ top: 0, behavior: "smooth" });
  }, []);
  const finishIntro = useCallback(() => { setIntro(false); sessionStorage.setItem("ghost-intro-seen", "1"); }, []);

  useEffect(() => {
    getListener();
    setDisplayName(localStorage.getItem("ghost-name") || "Ghost listener");
    setGhostMode(localStorage.getItem("ghost-reactive") !== "off");
    if (!sessionStorage.getItem("ghost-intro-seen") && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) { setIntro(true); sessionStorage.setItem("ghost-intro-seen", "1"); }
    void loadLibrary().catch(error => {
      const saved = localStorage.getItem("ghost-library");
      if (saved) { try { const data: LibraryData = JSON.parse(saved); setTracks(data.tracks); setPlaylists(data.playlists); setLiked(data.liked); setHistory(data.history); setDownloaded(data.downloaded); } catch {} }
      notify(navigator.onLine ? errorMessage(error) : "You're offline. Your saved music is still here.", navigator.onLine ? "error" : "success");
    });
    void api<{ jobs: DownloadJob[] }>("/api/downloads").then(data => setJobs(data.jobs)).catch(() => {});
    const playlistId = new URLSearchParams(window.location.search).get("playlist");
    if (playlistId) {
      const mix = mixes.find(item => item.id === playlistId);
      if (mix) navigate(`mix:${mix.id}`);
      else void api<{ playlist: Playlist; tracks: Track[] }>(`/api/playlists/${encodeURIComponent(playlistId)}`).then(data => { setSharedPlaylist(data.playlist); setTracks(items => [...items, ...data.tracks.filter(track => !items.some(t => t.id === track.id))]); navigate("Shared"); }).catch(error => notify(errorMessage(error), "error"));
    }
    if ("serviceWorker" in navigator) void navigator.serviceWorker.register("/sw.js").catch(() => {});
    const connection = () => setOffline(!navigator.onLine); connection();
    window.addEventListener("online", connection); window.addEventListener("offline", connection);
    const keydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") { event.preventDefault(); searchInput.current?.focus(); }
      const target = event.target as HTMLElement;
      if (["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName) || target.isContentEditable || document.querySelector('[role="dialog"]')) return;
      if (event.code === "Space") { event.preventDefault(); playerRef.current.toggle(); }
      if (event.key === "ArrowRight") { event.preventDefault(); playerRef.current.seek(playerRef.current.time + 5); }
      if (event.key === "ArrowLeft") { event.preventDefault(); playerRef.current.seek(playerRef.current.time - 5); }
      if (event.key === "Escape") { setQueueOpen(false); setMobileOpen(false); }
    };
    window.addEventListener("keydown", keydown);
    return () => { window.removeEventListener("keydown", keydown); window.removeEventListener("online", connection); window.removeEventListener("offline", connection); if (toastTimer.current) clearTimeout(toastTimer.current); searchAbort.current?.abort(); };
  }, [api, getListener, loadLibrary, navigate, notify]);

  function openModal(type: ModalType) { setFormError(""); setModal(type); }
  function setReactive(value: boolean) { setGhostMode(value); localStorage.setItem("ghost-reactive", value ? "on" : "off"); }
  function goHistory(direction: number) { const index = navigation.index + direction; if (index < 0 || index >= navigation.items.length) return; setNavigation({ ...navigation, index }); setView(navigation.items[index]); scrollArea.current?.scrollTo({ top: 0 }); }
  const getTracks = (ids: string[]) => ids.map(id => tracks.find(track => track.id === id) || catalog.find(track => track.id === id)).filter((track): track is Track => !!track);
  const activePlaylist = view.startsWith("mix:") ? mixes.find(mix => mix.id === view.slice(4)) : view.startsWith("playlist:") ? playlists.find(list => list.id === view.slice(9)) : view === "Shared" ? sharedPlaylist : null;
  const playlistSongs = activePlaylist ? getTracks(activePlaylist.trackIds) : [];
  const visibleMixes = mood === "For you" ? mixes : mixes.filter(mix => moodMixes[mood]?.includes(mix.id));
  const moodTracks = mood === "For you" ? catalog : getTracks([...new Set(visibleMixes.flatMap(mix => mix.trackIds))]);
  const likedTracks = getTracks(liked);
  const historyTracks = getTracks(history);

  function playFrom(track: Track, list?: Track[]) { if (player.current.id === track.id && (player.playing || player.time > 0)) { if (list) player.setQueue(list); player.toggle(); } else player.playTrack(track, list); }
  function playPlaylist(playlist: Playlist) { const list = getTracks(playlist.trackIds); if (list.length) player.playTrack(list[0], list); else notify("This playlist is waiting for its first track."); }
  function details(track: Track) { setDetailTrack(track); openModal("track"); }

  async function toggleLike(track: Track) {
    const active = !liked.includes(track.id);
    setLiked(items => active ? [...items, track.id] : items.filter(id => id !== track.id));
    try { await api("/api/library", { method: "POST", body: JSON.stringify({ action: "like", trackId: track.id, active }) }); if (active) notify(`“${track.title}” is in your liked songs.`); }
    catch (error) { setLiked(items => active ? items.filter(id => id !== track.id) : [...items, track.id]); notify(errorMessage(error), "error"); }
  }

  async function runSearch(event?: FormEvent, source?: string) {
    event?.preventDefault();
    if (!search.trim()) { searchInput.current?.focus(); return; }
    searchAbort.current?.abort(); const controller = new AbortController(); searchAbort.current = controller;
    setSearchTerm(search.trim()); setSearching(true); setSearchError(""); setSearchResults([]); navigate("Search");
    try { const result = await api<{ tracks: Track[] }>(`/api/search?q=${encodeURIComponent(search.trim())}${source ? `&source=${source}` : ""}`, { signal: controller.signal }); setSearchResults(result.tracks); setTracks(items => [...items, ...result.tracks.filter(track => !items.some(t => t.id === track.id))]); }
    catch (error) { if ((error as Error).name !== "AbortError") setSearchError(errorMessage(error)); }
    finally { if (!controller.signal.aborted) setSearching(false); }
  }

  async function createPlaylist(event: FormEvent) {
    event.preventDefault(); setFormBusy(true); setFormError("");
    try { const result = await api<{ id: string }>("/api/library", { method: "POST", body: JSON.stringify({ action: "create-playlist", name: playlistName, description: playlistDescription, trackIds: addTrackId ? [addTrackId] : [] }) }); await loadLibrary(); setModal(null); setPlaylistName(""); setPlaylistDescription(""); setAddTrackId(null); navigate(`playlist:${result.id}`); notify("A new little universe. Playlist created."); }
    catch (error) { setFormError(errorMessage(error)); } finally { setFormBusy(false); }
  }
  async function importPlaylist(event: FormEvent) {
    event.preventDefault(); setFormBusy(true); setFormError("");
    try { const result = await api<{ playlistId: string; count: number }>("/api/import", { method: "POST", body: JSON.stringify({ url: importUrl }) }); await loadLibrary(); setModal(null); setImportUrl(""); navigate(`playlist:${result.playlistId}`); notify(`${result.count} ${result.count === 1 ? "track" : "tracks"} crossed over to Ghost Music.`); }
    catch (error) { setFormError(errorMessage(error)); } finally { setFormBusy(false); }
  }
  async function addToPlaylist(playlistId: string) {
    if (!playlistId) return;
    try { await api("/api/library", { method: "POST", body: JSON.stringify({ action: "add-to-playlist", playlistId, trackId: detailTrack.id }) }); await loadLibrary(); notify(`Added to ${playlists.find(p => p.id === playlistId)?.name || "your playlist"}.`); setModal(null); }
    catch (error) { notify(errorMessage(error), "error"); }
  }
  async function removeFromPlaylist() {
    if (!activePlaylist || !view.startsWith("playlist:")) return;
    try { await api("/api/library", { method: "POST", body: JSON.stringify({ action: "remove-from-playlist", playlistId: activePlaylist.id, trackId: detailTrack.id }) }); await loadLibrary(); setModal(null); notify("Removed from this playlist. Still in your orbit."); }
    catch (error) { notify(errorMessage(error), "error"); }
  }
  function sharePlaylist(playlist?: Playlist) { setShareTarget({ name: playlist?.name || "Ghost Music", url: `${window.location.origin}${playlist ? `/?playlist=${encodeURIComponent(playlist.id)}` : "/"}` }); openModal("share"); }
  async function copyShare() { try { await navigator.clipboard.writeText(shareTarget.url); notify("Link copied. Share a little good music."); } catch { notify("Select the link above and copy it to share.", "error"); } }
  async function deletePlaylist() {
    if (!activePlaylist) return;
    setFormBusy(true);
    try { await api("/api/library", { method: "POST", body: JSON.stringify({ action: "delete-playlist", playlistId: activePlaylist.id }) }); await loadLibrary(); setModal(null); navigate("Library"); notify("Playlist deleted. The music is still here."); }
    catch (error) { setFormError(errorMessage(error)); } finally { setFormBusy(false); }
  }

  async function downloadTrack(track: Track, permitted = false) {
    if (track.source === "youtube" && !permitted) { setDetailTrack(track); openModal("permission"); return; }
    if (downloadLock.current) { setDownloadPanel(true); notify("One little ghost at a time. Your current download is on its way."); return; }
    downloadLock.current = true; setDownloadPanel(true); setQueueOpen(false);
    setTransfer({ track, progress: 0, bytes: 0, total: 0, status: "queued" });
    try {
      let { job } = await api<{ job: DownloadJob }>("/api/downloads", { method: "POST", body: JSON.stringify({ trackId: track.id, permitted }) });
      let count = 0;
      while (!["ready", "error"].includes(job.status)) {
        if (++count > 300) throw new Error("This extraction is taking too long. Check Downloads and try again.");
        setTransfer({ track, progress: job.progress, bytes: job.downloadedBytes, total: job.totalBytes, status: job.status });
        await new Promise(resolve => setTimeout(resolve, 850));
        const result = await api<{ jobs: DownloadJob[] }>("/api/downloads"); setJobs(result.jobs);
        const updated = result.jobs.find(item => item.id === job.id); if (updated) job = updated;
      }
      if (job.status === "error") throw new Error(job.error || "This track couldn't be downloaded.");
      setTransfer({ track, progress: 0, bytes: 0, total: job.totalBytes, status: "saving", metadata: job.metadata });
      const response = await fetch(`/api/downloads/${job.id}/file`, { headers: { "x-ghost-listener": getListener() } });
      if (!response.ok || !response.body) throw new Error("The download file isn't ready. Please retry.");
      const reader = response.body.getReader(); const chunks: BlobPart[] = []; let bytes = 0;
      const total = Number(response.headers.get("content-length")) || job.totalBytes;
      while (true) { const chunk = await reader.read(); if (chunk.done) break; chunks.push(new Uint8Array(chunk.value)); bytes += chunk.value.byteLength; setTransfer({ track, progress: total ? Math.round(bytes / total * 100) : 0, bytes, total, status: "saving", metadata: job.metadata }); }
      const blob = new Blob(chunks, { type: "audio/mpeg" });
      if ("caches" in window) { try { const cache = await caches.open("ghost-audio-v1"); await cache.put(track.audioPath || `/api/stream/${track.id}`, new Response(blob, { headers: { "Content-Type": "audio/mpeg", "Content-Length": String(blob.size) } })); } catch {} }
      const filename = `${track.artist} - ${track.title}.mp3`;
      if (await saveToDevice(chunks, filename)) notify(`“${track.title}” landed in Music/Ghost Music.`);
      else { const url = URL.createObjectURL(blob); const anchor = document.createElement("a"); anchor.href = url; anchor.download = filename; document.body.appendChild(anchor); anchor.click(); anchor.remove(); setTimeout(() => URL.revokeObjectURL(url), 30000); }
      setDownloaded(items => [...new Set([...items, track.id])]);
      setTransfer({ track, progress: 100, bytes: blob.size, total: blob.size, status: "complete", metadata: job.metadata });
      setJobs(items => [job, ...items.filter(item => item.id !== job.id)]);
      await loadLibrary(); notify(`“${track.title}” is yours to take offline.`);
    } catch (error) { setTransfer(old => old ? { ...old, status: "error", error: errorMessage(error) } : null); notify(errorMessage(error), "error"); }
    finally { downloadLock.current = false; }
  }

  function renderTracks(list: Track[], full = false) {
    return <div className={full ? "track-list" : "quick-picks"}>{full && list.length > 0 && <div className="track-table-head"><span>#</span><span>Title</span><span>Album</span><Clock3 size={14} /><span /></div>}{list.map((track, index) => <TrackRow key={track.id} track={track} index={index} full={full} current={player.current.id === track.id} playing={player.playing} liked={liked.includes(track.id)} onPlay={() => playFrom(track, list)} onLike={() => void toggleLike(track)} onDownload={() => void downloadTrack(track)} onDetails={() => details(track)} />)}</div>;
  }
  function emptyState(title: string, description: string, action = "Explore music") { return <div className="empty-state"><GhostIcon size={68} headphones /><h3>{title}</h3><p>{description}</p><button className="button primary" onClick={() => navigate("Explore")}><Compass size={16} />{action}</button></div>; }
  const moodFilters = <div className="mood-bar"><div className="mood-tabs">{moodNames.map(name => <button key={name} className={`mood-chip ${mood === name ? "selected" : ""}`} onClick={() => setMood(name)} aria-pressed={mood === name}>{name === "For you" && <Sparkles size={13} />}{name}</button>)}</div><button className="surprise-button" onClick={() => { const track = catalog[Math.floor(Math.random() * catalog.length)]; player.playTrack(track, catalog); notify(`A little surprise: ${track.title}.`); }}><Shuffle size={14} /><span>Surprise me</span></button></div>;
  const mixSection = <section className="mood-section"><div className="section-heading"><div><h2>{view === "Explore" ? "A world of good spirits" : "Made for your mood"}<span className="heading-dot" /></h2><p>A different feeling. The same good music.</p></div><button className="text-link" onClick={() => { setMood("For you"); navigate(view === "Explore" ? "Library" : "Explore"); }}>{view === "Explore" ? "Your library" : "View all"}<ArrowUpRight size={15} /></button></div><div className="mix-grid">{visibleMixes.map((mix, index) => <MixCard key={mix.id} mix={mix} index={index} onOpen={() => navigate(`mix:${mix.id}`)} onPlay={() => playPlaylist(mix)} />)}</div></section>;

  return <div className="app-shell" data-reactive={ghostMode}>
    {mobileOpen && <button className="sidebar-backdrop" onClick={() => setMobileOpen(false)} aria-label="Close navigation" />}
    <aside className={`sidebar ${mobileOpen ? "sidebar-open" : ""}`}>
      <button className="brand" onClick={() => navigate("Home")} aria-label="Ghost Music home"><GhostIcon size={42} /><span><strong>GHOST</strong><small>MUSIC</small></span></button>
      <nav className="primary-nav" aria-label="Main navigation">{[{ name: "Home", icon: Home }, { name: "Explore", icon: Compass }, { name: "Library", label: "Your library", icon: Library }].map(({ name, label, icon: Icon }) => <button key={name} onClick={() => navigate(name)} className={`nav-item ${view === name ? "active" : ""}`}><Icon size={19} strokeWidth={1.7} /><span>{label || name}</span>{view === name && <span className="nav-active-dot" />}</button>)}</nav>
      <div className="sidebar-section"><h3>YOUR COLLECTION</h3><nav aria-label="Music collection"><button className={`nav-item ${view === "Liked" ? "active" : ""}`} onClick={() => navigate("Liked")}><Heart size={18} strokeWidth={1.7} /><span>Liked songs</span>{liked.length > 0 && <small className="nav-count">{liked.length}</small>}</button><button className={`nav-item ${view === "Downloads" ? "active" : ""}`} onClick={() => navigate("Downloads")}><Download size={18} strokeWidth={1.7} /><span>Downloads</span>{downloadLock.current ? <span className="activity-dot" /> : downloaded.length > 0 && <small className="nav-count">{downloaded.length}</small>}</button><button className={`nav-item ${view === "History" ? "active" : ""}`} onClick={() => navigate("History")}><History size={18} strokeWidth={1.7} /><span>Recently played</span></button></nav></div>
      <div className="sidebar-section playlist-sidebar"><div className="sidebar-section-title"><h3>YOUR PLAYLISTS</h3><button className="icon-button" onClick={() => { setAddTrackId(null); openModal("create"); }} aria-label="Create playlist"><Plus size={17} /></button></div><div className="sidebar-playlists">{playlists.map(playlist => <button key={playlist.id} className={`sidebar-playlist ${view === `playlist:${playlist.id}` ? "selected" : ""}`} onClick={() => navigate(`playlist:${playlist.id}`)}><img src={playlist.artwork} alt="" /><span><strong>{playlist.name}</strong><small>Playlist · {playlist.trackIds.length} tracks</small></span></button>)}</div><button className="new-playlist" onClick={() => { setAddTrackId(null); openModal("create"); }}><Plus size={15} />Create a playlist</button></div>
      <div className="sidebar-bottom"><div className="ghost-mode-card"><div className="ghost-mode-title"><GhostIcon size={22} /><span>Ghost mode</span><button className={`toggle-switch ${ghostMode ? "on" : ""}`} role="switch" aria-checked={ghostMode} aria-label="Beat-reactive Ghost mode" onClick={() => setReactive(!ghostMode)}><span /></button></div><p>Feel the music. See the energy.</p><span className="mode-caption">BEAT-REACTIVE VISUALS</span></div><button className="sidebar-footer" onClick={() => sharePlaylist()}><span className="little-status" />Good music. Better company.<ArrowUpRight size={12} /></button></div>
    </aside>

    <div className="workspace">
      <header className="topbar"><button className="icon-button mobile-menu" onClick={() => setMobileOpen(true)} aria-label="Open navigation"><Menu size={22} /></button><div className="history-controls"><button className="icon-button" onClick={() => goHistory(-1)} disabled={navigation.index === 0} aria-label="Go back"><ChevronLeft size={20} /></button><button className="icon-button" onClick={() => goHistory(1)} disabled={navigation.index === navigation.items.length - 1} aria-label="Go forward"><ChevronRight size={20} /></button></div><form className="search-field" onSubmit={runSearch}><Search size={18} /><input ref={searchInput} value={search} onChange={event => setSearch(event.target.value)} placeholder="Search songs, artists, or paste a YouTube link" aria-label="Search songs, artists, or a YouTube link" />{search ? <button type="button" className="search-clear" onClick={() => { setSearch(""); searchInput.current?.focus(); }} aria-label="Clear search"><X size={15} /></button> : <kbd>⌘ K</kbd>}</form><div className="header-right">{offline && <span className="offline-status" title="Offline · saved music is available"><WifiOff size={17} /></span>}<button className="icon-button preferences-button" onClick={() => openModal("settings")} aria-label="Listening preferences"><SlidersHorizontal size={18} /></button><span className="header-divider" /><button className="profile-button" onClick={() => openModal("settings")}><span className="profile-avatar">{displayName[0]?.toUpperCase() || "G"}<i /></span><span className="profile-copy"><small>IN GOOD SPIRITS</small><strong>{displayName}</strong></span><ChevronDown size={14} /></button></div></header>
      <main className="main-scroll" ref={scrollArea} id="main-content"><div className="main-content" key={view}>
        {view === "Home" && <>
          <div className="hero-grid"><section className="hero"><img className="hero-background" src="/images/ghost-hero.png" alt="A friendly ghost wearing headphones in a crimson cloud" /><div className="hero-shade" /><div className="hero-copy"><span className="eyebrow"><span className="live-dot" />A NEW HOME FOR YOUR SOUND</span><h1>Let the music<br />possess you.</h1><p>Discover new favorites. Bring your own obsessions.</p><div className="hero-actions"><button className="button primary" onClick={() => player.playTrack(catalog[0], catalog)}><Play size={15} fill="currentColor" />Start listening</button><button className="button secondary" onClick={() => openModal("import")}><Youtube size={18} />Import playlist</button></div></div><div className="hero-footnote"><span className="sound-wave"><i /><i /><i /><i /><i /></span> GOOD TUNES. NO STRINGS.</div></section><section className="offline-card"><div className="offline-eyebrow"><Download size={12} />THE OFFLINE SIDE</div><div className="offline-art"><div className="orbit-ring ring-one" /><div className="orbit-ring ring-two" /><GhostIcon size={77} headphones /><Music2 size={17} className="offline-note" /><span className="offline-check"><Check size={12} /></span></div><div className="offline-copy"><h2>No signal.<br />Still a vibe.</h2><p>Your favorites, wherever you disappear.</p><button className="text-link" onClick={() => navigate("Downloads")}>Explore downloads<ArrowUpRight size={15} /></button></div></section></div>
          {moodFilters}{mixSection}
          <section className="quick-section"><div className="section-heading"><div><h2>On your frequency <Radio size={17} className="accent-icon" /></h2><p>Fresh finds. Future favorites.</p></div><button className="text-link" onClick={() => player.playTrack(moodTracks[0], moodTracks)}><Play size={12} fill="currentColor" />Play all</button></div>{renderTracks(moodTracks.slice(0, 6))}</section>
          <div className="discovery-banner"><div className="banner-ghost"><GhostIcon size={38} headphones /></div><div><h3>Good music deserves good company.</h3><p>Send a playlist to a friend. Start a new obsession.</p></div><button className="button secondary" onClick={() => sharePlaylist(mixes[0])}>Share the feeling<ArrowUpRight size={15} /></button></div>
          <div className="content-footer"><GhostIcon size={15} /><span>Original sounds. Unlimited good spirits.</span><span>GHOST MUSIC © {new Date().getFullYear()}</span></div>
        </>}

        {view === "Explore" && <><div className="page-heading"><span className="eyebrow">GO A LITTLE OFF THE BEATEN TRACK</span><h1>Find your next obsession.</h1><p>New sounds, late-night discoveries, and everything in between.</p></div>{moodFilters}{mixSection}<section className="quick-section"><div className="section-heading"><div><h2>Ghost Sessions originals</h2><p>Made for this little corner of the internet. Free to stream and download.</p></div><button className="text-link" onClick={() => openModal("import")}><Youtube size={16} />Bring your own</button></div>{renderTracks(moodTracks, true)}</section></>}

        {view === "Library" && <><div className="page-heading with-action"><div><span className="eyebrow">YOUR CORNER OF THE OTHER SIDE</span><h1>Your library</h1><p>Every obsession, in one place.</p></div><button className="button primary" onClick={() => { setAddTrackId(null); openModal("create"); }}><Plus size={16} />New playlist</button></div><div className="library-shortcuts"><button className="library-shortcut liked-shortcut" onClick={() => navigate("Liked")}><span><Heart size={30} fill="currentColor" /></span><div><h3>Liked songs</h3><p>{liked.length} tracks that hit different</p></div><ArrowUpRight size={22} /></button><button className="library-shortcut downloads-shortcut" onClick={() => navigate("Downloads")}><span><Download size={30} /></span><div><h3>Offline favorites</h3><p>{downloaded.length} tracks ready to go</p></div><ArrowUpRight size={22} /></button></div><div className="library-tabs">{["Playlists", "Tracks"].map(tab => <button className={libraryTab === tab ? "selected" : ""} key={tab} onClick={() => setLibraryTab(tab)}>{tab}</button>)}</div>{libraryTab === "Playlists" ? <div className="mix-grid library-grid">{playlists.map((playlist, index) => <MixCard key={playlist.id} mix={playlist} index={index} onOpen={() => navigate(`playlist:${playlist.id}`)} onPlay={() => playPlaylist(playlist)} />)}<button className="create-playlist-card" onClick={() => { setAddTrackId(null); openModal("create"); }}><Plus size={30} /><span>Create your next obsession</span></button></div> : renderTracks(getTracks([...new Set([...liked, ...playlists.flatMap(list => list.trackIds)])]), true)}</>}

        {(view === "Liked" || view === "History") && <><div className="page-heading collection-heading"><div className={`collection-icon ${view === "Liked" ? "hearts" : "recent"}`}>{view === "Liked" ? <Heart size={46} fill="currentColor" /> : <History size={46} />}</div><div><span className="eyebrow">YOUR COLLECTION</span><h1>{view === "Liked" ? "Liked songs" : "Recently played"}</h1><p>{view === "Liked" ? "The ones that stay with you." : "A little trip down your musical memory lane."} · {view === "Liked" ? liked.length : history.length} tracks</p></div></div>{(view === "Liked" ? likedTracks : historyTracks).length ? <><div className="collection-actions"><button className="button primary" onClick={() => { const list = view === "Liked" ? likedTracks : historyTracks; player.playTrack(list[0], list); }}><Play size={16} fill="currentColor" />Play all</button><span>Always close to your heart. Never far from your ears.</span></div>{renderTracks(view === "Liked" ? likedTracks : historyTracks, true)}</> : emptyState(view === "Liked" ? "Your first obsession is out there." : "Every great story starts with a song.", view === "Liked" ? "Tap the heart on any song and give it a home right here." : "Start listening and your recent tracks will appear here.")}</>}

        {activePlaylist && <><div className="playlist-hero"><img src={activePlaylist.artwork} alt={`${activePlaylist.name} cover`} /><div><span className="eyebrow">{view.startsWith("mix:") ? "A GHOST MUSIC MIX" : view === "Shared" ? "SHARED WITH GOOD SPIRITS" : "YOUR PLAYLIST"}</span><h1>{activePlaylist.name}</h1><p>{activePlaylist.description || "A little collection of your favorite hauntings."}</p><span className="playlist-summary"><GhostIcon size={17} />Ghost Music<span>·</span>{playlistSongs.length} tracks<span>·</span>{Math.round(playlistSongs.reduce((sum, track) => sum + track.duration, 0) / 60)} min</span></div></div><div className="playlist-actions"><button className="button primary" onClick={() => playPlaylist(activePlaylist)} disabled={!playlistSongs.length}><Play size={16} fill="currentColor" />Play it all</button><button className="icon-button" onClick={() => sharePlaylist(activePlaylist)} aria-label="Share playlist" title="Share playlist"><Link2 size={20} /></button>{view.startsWith("playlist:") && <button className="icon-button" onClick={() => openModal("delete")} aria-label="Delete playlist"><Trash2 size={19} /></button>}<span className="playlist-track-count">{playlistSongs.length} tracks. One good feeling.</span></div>{playlistSongs.length ? renderTracks(playlistSongs, true) : emptyState("A quiet little haunt.", "Find a track you love, open its options, and add it to this playlist.")}</>}

        {view === "Search" && <><div className="page-heading"><span className="eyebrow">FOLLOW THE SOUND</span><h1>Results for <span className="search-term">“{searchTerm}”</span></h1><p>{searching ? "Looking for your next favorite…" : `${searchResults.length} tracks found`}</p></div>{searching ? <div className="search-loading"><GhostIcon size={66} headphones /><h3>Searching the other side…</h3><p>Finding tracks and collecting their details.</p></div> : searchError ? <div className="empty-state"><WifiOff size={42} /><h3>A little interference.</h3><p>{searchError}</p><button className="button primary" onClick={() => void runSearch()}>Try again<RotateCcw size={15} /></button></div> : searchResults.length ? <><div className="search-actions"><button className="button primary" onClick={() => player.playTrack(searchResults[0], searchResults)}><Play size={15} fill="currentColor" />Play results</button><button className="button secondary" onClick={() => void runSearch(undefined, "youtube")}><Youtube size={17} />Search YouTube</button></div>{renderTracks(searchResults, true)}</> : emptyState("No ghosts here. Yet.", "Try another song, artist, genre, or a public YouTube link.")}</>}

        {view === "Downloads" && <><div className="page-heading with-action"><div><span className="eyebrow">NO SIGNAL. NO PROBLEM.</span><h1>Your music. Off the grid.</h1><p>Real MP3 files. Full metadata. Yours to take along.</p></div><span className="downloads-count"><HardDrive size={17} />{downloaded.length} saved tracks</span></div><div className="download-info"><ShieldCheck size={22} /><div><strong>Good music, no strings attached.</strong><p>Ghost Sessions originals are free to save. Only download imported music you own or have permission to use.</p></div></div>{transfer && !["complete", "error"].includes(transfer.status) && <button className="active-download-banner" onClick={() => setDownloadPanel(true)}><GhostIcon className="float" size={32} /><span><strong>One little ghost is bringing your music.</strong><small>{transfer.track.title} · {transfer.progress}%</small></span><ArrowUpRight size={20} /></button>}{jobs.some(job => job.status === "ready") ? <div className="download-list">{[...new Map(jobs.filter(job => job.status === "ready").map(job => [job.trackId, job])).values()].map(job => { const track = tracks.find(item => item.id === job.trackId); return track ? <div className="download-item" key={job.id}><button className="download-art" onClick={() => playFrom(track)}><img src={track.artwork} alt="" /><Play size={20} fill="currentColor" /></button><div className="download-track-info"><strong>{track.title}</strong><span>{track.artist} · {track.album}</span><div className="metadata-tags"><span>MP3</span><span>{job.metadata?.bitrate || "192 kbps"}</span><span>{sizeLabel(job.totalBytes)}</span><span><Check size={11} />Metadata included</span></div></div><span className="offline-ready"><CheckCircle2 size={15} />Ready for offline</span><button className="icon-button" onClick={() => void downloadTrack(track)} aria-label={`Save ${track.title} again`} title="Save MP3 again"><Download size={20} /></button><button className="icon-button" onClick={() => details(track)} aria-label={`Details for ${track.title}`}><MoreHorizontal size={20} /></button></div> : null; })}</div> : downloaded.length ? renderTracks(getTracks(downloaded), true) : emptyState("Take your favorite ghosts with you.", "Tap the download icon on any track. We'll save the audio, its artwork, and all the little details.", "Find your first download")}<div className="section-heading download-recommendation"><div><h2>A good place to start</h2><p>Original Ghost Sessions, ready for the road.</p></div></div>{renderTracks(catalog.slice(0, 4))}</>}
      </div></main>
    </div>

    <PlayerBar player={player} liked={liked.includes(player.current.id)} onLike={() => void toggleLike(player.current)} onDownload={() => void downloadTrack(player.current)} onQueue={() => { setQueueOpen(!queueOpen); setDownloadPanel(false); }} onExpand={() => { setDetailTrack(player.current); openModal("now-playing"); }} queueOpen={queueOpen} />

    {queueOpen && <aside className="queue-panel"><div className="panel-header"><div><span className="eyebrow">LET IT KEEP HAUNTING YOU</span><h3>Your orbit <span>{player.queue.length}</span></h3></div><button className="icon-button" onClick={() => setQueueOpen(false)} aria-label="Close queue"><X size={19} /></button></div><div className="queue-list">{player.queue.map((track, index) => <button className={`queue-track ${track.id === player.current.id ? "current" : ""}`} key={`${track.id}-${index}`} onClick={() => player.playTrack(track)}><span className="queue-number">{track.id === player.current.id ? <span className="equalizer"><i /><i /><i /></span> : index + 1}</span><img src={track.artwork} alt="" /><span><strong>{track.title}</strong><small>{track.artist}</small></span><span className="queue-duration">{formatTime(track.duration)}</span></button>)}</div><div className="queue-footer"><Headphones size={14} />A little lost in the music. Just right.</div></aside>}

    {downloadPanel && transfer && <aside className="download-panel" aria-live="polite"><div className="panel-header"><div className="delivery-heading"><GhostIcon size={26} /><h3>Ghost delivery</h3></div><button className="icon-button" onClick={() => setDownloadPanel(false)} aria-label="Hide download progress"><X size={18} /></button></div><div className="delivery-track"><img src={transfer.track.artwork} alt="" /><div><strong>{transfer.track.title}</strong><span>{transfer.track.artist}</span></div>{transfer.status === "complete" && <CheckCircle2 size={24} className="success-icon" />}</div><h4>{transfer.status === "complete" ? "Yours. Even offline." : transfer.status === "error" ? "This ghost hit a wall." : transfer.status === "processing" ? "Adding all the little details…" : transfer.status === "saving" ? "Bringing your music home…" : "Extracting the good stuff…"}</h4>{transfer.status === "error" ? <p className="delivery-error">{transfer.error}</p> : <><div className={`ghost-progress ${transfer.status === "queued" ? "indeterminate" : ""}`}><div style={{ width: `${transfer.progress}%` }} /><GhostIcon size={24} style={{ left: `clamp(0px, ${transfer.progress}%, calc(100% - 15px))` }} /></div><div className="delivery-progress-label"><span>{transfer.status === "complete" ? "MP3 · Artwork & metadata included" : transfer.total ? `${sizeLabel(transfer.bytes)} of ${sizeLabel(transfer.total)}` : "Connecting to the source"}</span><strong>{transfer.status === "complete" ? <Check size={14} /> : `${transfer.progress}%`}</strong></div></>}{transfer.metadata && <div className="delivery-metadata"><span>{transfer.metadata.format} · {transfer.metadata.bitrate}</span><span>{sizeLabel(transfer.total)}</span></div>}<button className="text-link" onClick={() => { setDownloadPanel(false); navigate("Downloads"); }}>All your downloads<ArrowRight size={15} /></button>{transfer.status === "error" && <button className="button secondary retry-download" onClick={() => void downloadTrack(transfer.track, true)}><RotateCcw size={14} />Try again</button>}</aside>}

    {toast && <div className={`toast ${toast.kind === "error" ? "toast-error" : ""}`} role="status">{toast.kind === "error" ? <AlertCircle size={19} /> : <CheckCircle2 size={19} />}<span>{toast.message}</span><button onClick={() => setToast(null)} aria-label="Dismiss notification"><X size={15} /></button></div>}

    {modal === "import" && <Modal title="Bring your favorites to the other side." subtitle="A song, a playlist, a whole new obsession. All it takes is a link." onClose={() => { if (!formBusy) setModal(null); }}><div className="import-visual"><span><Youtube size={30} /></span><span className="import-dots">······<ArrowRight size={18} />······</span><span><GhostIcon size={38} headphones /></span></div><form onSubmit={importPlaylist}><label className="field-label" htmlFor="youtube-url">YouTube song or playlist link</label><div className="input-with-icon"><Link2 size={17} /><input id="youtube-url" type="url" value={importUrl} onChange={event => setImportUrl(event.target.value)} placeholder="https://www.youtube.com/playlist?list=…" required autoFocus disabled={formBusy} /></div><p className="field-hint">Public videos and playlists · Up to 30 tracks per import</p><div className="import-features"><span><Check size={14} />Original track details</span><span><Check size={14} />Artwork included</span><span><Check size={14} />Your very own playlist</span></div>{formError && <div className="form-error"><AlertCircle size={17} />{formError}</div>}<button className="button primary full-width" type="submit" disabled={formBusy}>{formBusy ? <><LoaderCircle size={17} className="spin" />Finding your tracks…</> : <><GhostIcon size={19} />Import to Ghost Music</>}</button><p className="modal-note">Playback and extraction depend on source availability. Private, age-restricted, or region-blocked videos may not be supported.</p></form></Modal>}

    {modal === "create" && <Modal title="Make room for a new obsession." subtitle="Your songs. Your order. Your little universe." onClose={() => { if (!formBusy) setModal(null); }}><form onSubmit={createPlaylist}><div className="create-art"><GhostIcon size={52} headphones /><Music2 size={22} /></div><label className="field-label" htmlFor="playlist-name">Playlist name</label><input className="form-input" id="playlist-name" value={playlistName} onChange={event => setPlaylistName(event.target.value)} placeholder="e.g. Songs for the other side" maxLength={80} required autoFocus /><label className="field-label" htmlFor="playlist-description">A little description <span>(optional)</span></label><textarea className="form-input" id="playlist-description" value={playlistDescription} onChange={event => setPlaylistDescription(event.target.value)} placeholder="What's the feeling?" maxLength={400} rows={3} />{addTrackId && <p className="field-hint"><Check size={13} />Your selected track will be added automatically.</p>}{formError && <div className="form-error">{formError}</div>}<button type="submit" className="button primary full-width" disabled={formBusy || !playlistName.trim()}>{formBusy ? <LoaderCircle size={16} className="spin" /> : <Plus size={17} />}Create playlist</button></form></Modal>}

    {modal === "settings" && <Modal title="Your kind of Ghost." subtitle="A few little things to make this place feel like you." onClose={() => setModal(null)}><label className="field-label" htmlFor="listener-name">What should we call you?</label><input className="form-input" id="listener-name" value={displayName} maxLength={30} onChange={event => { setDisplayName(event.target.value); localStorage.setItem("ghost-name", event.target.value); }} /><div className="setting-row"><div><strong><GhostIcon size={20} />Ghost mode</strong><p>Let layers of color move with the actual audio.</p></div><button className={`toggle-switch ${ghostMode ? "on" : ""}`} role="switch" aria-checked={ghostMode} aria-label="Enable reactive colors" onClick={() => setReactive(!ghostMode)}><span /></button></div><div className="setting-row"><div><strong><Sparkles size={18} />The startup ritual</strong><p>One spin. Headphones on. Ten seconds of good spirits.</p></div><button className="icon-button" onClick={() => { setModal(null); setIntro(true); }} aria-label="Replay startup animation"><RotateCcw size={20} /></button></div><div className="shortcuts"><h4><Keyboard size={17} />A few shortcuts</h4><div><span>Find your next track</span><kbd>⌘ / Ctrl + K</kbd></div><div><span>Play / pause</span><kbd>Space</kbd></div><div><span>Skip back / forward 5 seconds</span><kbd>← / →</kbd></div></div><div className="about-ghost"><GhostIcon size={24} /><p><strong>Built for the music. Not the noise.</strong><br />Ghost Sessions are original instrumental demos, free to stream and download. Your library stays with this browser.</p></div></Modal>}

    {(modal === "track" || modal === "now-playing") && <Modal title={modal === "now-playing" ? "In good spirits." : "A little more about this one."} subtitle={modal === "now-playing" ? "Let everything else fade out." : "All the details. All the feeling."} onClose={() => setModal(null)}><div className={`track-detail ${modal === "now-playing" ? "expanded-track" : ""}`}><img src={detailTrack.artwork} alt={`${detailTrack.album} cover`} /><div><h3>{detailTrack.title}</h3><p>{detailTrack.artist}</p><span>{detailTrack.album}</span></div></div><div className="detail-actions"><button className="button primary" onClick={() => playFrom(detailTrack)}>{player.current.id === detailTrack.id && player.playing ? <Pause size={16} fill="currentColor" /> : <Play size={16} fill="currentColor" />}{player.current.id === detailTrack.id && player.playing ? "Pause" : "Play track"}</button><button className={`button secondary ${liked.includes(detailTrack.id) ? "is-liked" : ""}`} onClick={() => void toggleLike(detailTrack)}><Heart size={16} fill={liked.includes(detailTrack.id) ? "currentColor" : "none"} />{liked.includes(detailTrack.id) ? "Liked" : "Like"}</button><button className="button secondary" onClick={() => { setModal(null); void downloadTrack(detailTrack); }} aria-label="Download this track"><Download size={18} /></button></div><div className="track-detail-grid"><div><span>Duration</span><strong>{detailTrack.duration ? formatTime(detailTrack.duration) : "Available on playback"}</strong></div><div><span>Source</span><strong>{detailTrack.source === "ghost" ? "Ghost Sessions original" : "YouTube"}</strong></div><div><span>Genre</span><strong>{detailTrack.genre}</strong></div><div><span>Offline format</span><strong>MP3 · {detailTrack.source === "ghost" ? "160" : "192"} kbps</strong></div></div><label className="field-label" htmlFor="add-to-playlist"><FolderPlus size={15} />Add to your playlist</label><select className="form-input" id="add-to-playlist" defaultValue="" onChange={event => void addToPlaylist(event.target.value)}><option value="" disabled>Choose a little universe…</option>{playlists.map(playlist => <option key={playlist.id} value={playlist.id}>{playlist.name}</option>)}</select><button className="text-link new-playlist-link" onClick={() => { setAddTrackId(detailTrack.id); openModal("create"); }}><Plus size={14} />Or create a new playlist</button>{activePlaylist && view.startsWith("playlist:") && activePlaylist.trackIds.includes(detailTrack.id) && <button className="text-link remove-track-link" onClick={() => void removeFromPlaylist()}><Trash2 size={14} />Remove from this playlist</button>}{detailTrack.sourceUrl && <a className="source-link" href={detailTrack.sourceUrl} target="_blank" rel="noopener noreferrer"><Youtube size={16} />View original on YouTube<ArrowUpRight size={14} /></a>}{detailTrack.source === "ghost" && <p className="modal-note">An original synthesized instrumental made for Ghost Music. Free to listen, share, and save. Cover artwork and track metadata are embedded in every download.</p>}</Modal>}

    {modal === "share" && <Modal title="Good music travels." subtitle={`Send “${shareTarget.name}” to someone who gets your taste.`} onClose={() => setModal(null)}><div className="share-visual"><GhostIcon size={64} headphones /><span>+</span><GhostIcon size={48} /></div><label className="field-label" htmlFor="share-link">Your shareable link</label><div className="input-with-icon"><Link2 size={17} /><input id="share-link" value={shareTarget.url} readOnly onFocus={event => event.target.select()} /></div><button className="button primary full-width" onClick={() => void copyShare()}><Copy size={17} />Copy link</button><p className="modal-note">Anyone with the link can open and play this collection. No account or app download needed.</p></Modal>}

    {modal === "permission" && <Modal title="Keep the good spirits going." subtitle="A quick note before saving imported music." onClose={() => setModal(null)}><div className="permission-icon"><ShieldCheck size={44} /></div><p className="permission-copy">Only download audio you own or have the creator's permission to save. Source restrictions may prevent some downloads.</p><div className="permission-track"><img src={detailTrack.artwork} alt="" /><span><strong>{detailTrack.title}</strong><small>{detailTrack.artist}</small></span></div><button className="button primary full-width" onClick={() => { setModal(null); void downloadTrack(detailTrack, true); }}><Download size={17} />I have permission — download</button><button className="button secondary full-width" onClick={() => setModal(null)}>Keep streaming instead</button></Modal>}

    {modal === "delete" && <Modal title="Let this playlist disappear?" subtitle={`“${activePlaylist?.name}” will be deleted. Its tracks will still be available to play.`} onClose={() => setModal(null)}>{formError && <div className="form-error">{formError}</div>}<div className="dialog-actions"><button className="button secondary" onClick={() => setModal(null)}>Keep it around</button><button className="button primary" onClick={() => void deletePlaylist()} disabled={formBusy}>{formBusy ? <LoaderCircle size={16} className="spin" /> : <Trash2 size={16} />}Delete playlist</button></div></Modal>}
    {intro && <GhostIntro onDone={finishIntro} />}
  </div>;
}

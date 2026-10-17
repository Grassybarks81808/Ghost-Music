export type Track = {
  id: string; title: string; artist: string; album: string; duration: number;
  artwork: string; source: string; sourceUrl: string | null; audioPath: string | null; genre: string;
};
export type Playlist = { id: string; name: string; description: string; artwork: string; trackIds: string[]; listenerId?: string };
export type DownloadJob = {
  id: string; trackId: string; status: string; progress: number; downloadedBytes: number;
  totalBytes: number; error: string | null; metadata: Record<string, string | number> | null;
};

export const catalog: Track[] = [
  { id: "midnight-drive", title: "Midnight Drive", artist: "Ghost Sessions", album: "After Hours", duration: 162, artwork: "/images/after-hours.png", source: "ghost", sourceUrl: null, audioPath: "/audio/midnight-drive.mp3", genre: "Electronic" },
  { id: "weightless", title: "Weightless", artist: "Ghost Sessions", album: "Head in the Clouds", duration: 188, artwork: "/images/cloud-nine.png", source: "ghost", sourceUrl: null, audioPath: "/audio/weightless.mp3", genre: "Chill" },
  { id: "neon-dreams", title: "Neon Dreams", artist: "Ghost Sessions", album: "Neon Nostalgia", duration: 204, artwork: "/images/neon-dreams.png", source: "ghost", sourceUrl: null, audioPath: "/audio/neon-dreams.mp3", genre: "Electronic" },
  { id: "golden", title: "Golden", artist: "Ghost Sessions", album: "Indie State of Mind", duration: 176, artwork: "/images/indie-spirit.png", source: "ghost", sourceUrl: null, audioPath: "/audio/golden.mp3", genre: "Indie" },
  { id: "slow-motion", title: "Slow Motion", artist: "Ghost Sessions", album: "Deep Focus", duration: 192, artwork: "/images/deep-focus.png", source: "ghost", sourceUrl: null, audioPath: "/audio/slow-motion.mp3", genre: "Focus" },
  { id: "somewhere-somehow", title: "Somewhere, Somehow", artist: "Ghost Sessions", album: "Lowkey & Lo-fi", duration: 180, artwork: "/images/lofi.png", source: "ghost", sourceUrl: null, audioPath: "/audio/somewhere-somehow.mp3", genre: "Lo-fi" },
  { id: "lunar-tides", title: "Lunar Tides", artist: "Ghost Sessions", album: "After Hours", duration: 168, artwork: "/images/after-hours.png", source: "ghost", sourceUrl: null, audioPath: "/audio/lunar-tides.mp3", genre: "Chill" },
  { id: "blue-hour", title: "Blue Hour", artist: "Ghost Sessions", album: "Deep Focus", duration: 196, artwork: "/images/deep-focus.png", source: "ghost", sourceUrl: null, audioPath: "/audio/blue-hour.mp3", genre: "Focus" },
];

export const mixes: Playlist[] = [
  { id: "mix-after-hours", name: "After Hours", description: "For when the city falls asleep.", artwork: "/images/after-hours.png", trackIds: ["midnight-drive", "lunar-tides", "neon-dreams"] },
  { id: "mix-clouds", name: "Head in the Clouds", description: "A softer side of your day.", artwork: "/images/cloud-nine.png", trackIds: ["weightless", "lunar-tides", "slow-motion"] },
  { id: "mix-neon", name: "Neon Nostalgia", description: "Old souls. New frequencies.", artwork: "/images/neon-dreams.png", trackIds: ["neon-dreams", "midnight-drive", "golden"] },
  { id: "mix-indie", name: "Indie State of Mind", description: "A little off the beaten track.", artwork: "/images/indie-spirit.png", trackIds: ["golden", "somewhere-somehow", "weightless"] },
  { id: "mix-focus", name: "Deep Focus", description: "Less noise. More flow.", artwork: "/images/deep-focus.png", trackIds: ["slow-motion", "blue-hour", "weightless"] },
  { id: "mix-lofi", name: "Lowkey & Lo-fi", description: "Slow down. Settle in.", artwork: "/images/lofi.png", trackIds: ["somewhere-somehow", "golden", "lunar-tides"] },
];

export const initialPlaylists = [
  { name: "Late night drives", artwork: "/images/after-hours.png", trackIds: ["midnight-drive", "neon-dreams", "lunar-tides"] },
  { name: "Lost in the moment", artwork: "/images/cloud-nine.png", trackIds: ["weightless", "slow-motion"] },
  { name: "On repeat", artwork: "/images/indie-spirit.png", trackIds: ["golden", "somewhere-somehow"] },
];

export const formatTime = (seconds: number) => `${Math.floor((seconds || 0) / 60)}:${String(Math.floor((seconds || 0) % 60)).padStart(2, "0")}`;

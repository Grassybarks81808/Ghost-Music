// Generates Ghost Music's original artwork and icons with a tiny dependency-free
// PNG encoder. Produces the files referenced by src/lib/catalog.ts, the manifest,
// and the packaged desktop/Android apps.
import { mkdirSync, writeFileSync } from 'node:fs';
import zlib from 'node:zlib';

/* ------------------------------------------------------------------ PNG ---- */
const crcTable = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buffer) {
  let c = -1;
  for (let i = 0; i < buffer.length; i++) c = crcTable[(c ^ buffer[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

/** Encode straight RGBA bytes (0-255, non-premultiplied) as a PNG. */
function encodePNG(width, height, rgba) {
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0;
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // RGBA
  header[10] = 0;
  header[11] = 0;
  header[12] = 0;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/* --------------------------------------------------------------- painting -- */
const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const mix = (a, b, t) => a + (b - a) * clamp(t);
const smoothstep = (edge0, edge1, x) => {
  const t = clamp((x - edge0) / (edge1 - edge0));
  return t * t * (3 - 2 * t);
};
const hex = value => {
  const n = parseInt(value.replace('#', ''), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};
function noise(seed) {
  // Deterministic value noise, good enough for film grain.
  let s = seed >>> 0;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967295;
  };
}

class Canvas {
  constructor(width, height) {
    this.width = width;
    this.height = height;
    this.data = Buffer.alloc(width * height * 4);
    this.rand = noise(0x9e3779b9);
  }
  blend(x, y, r, g, b, a) {
    if (a <= 0 || x < 0 || y < 0 || x >= this.width || y >= this.height) return;
    const i = (y * this.width + x) * 4;
    const d = this.data;
    const src = clamp(a);
    const dstA = d[i + 3] / 255;
    const outA = src + dstA * (1 - src);
    d[i] = Math.round((r * src + d[i] * dstA * (1 - src)) / (outA || 1));
    d[i + 1] = Math.round((g * src + d[i + 1] * dstA * (1 - src)) / (outA || 1));
    d[i + 2] = Math.round((b * src + d[i + 2] * dstA * (1 - src)) / (outA || 1));
    d[i + 3] = Math.round(outA * 255);
  }
  /** Paint every pixel through a shader returning [r,g,b,a]. */
  shade(fn) {
    for (let y = 0; y < this.height; y++) {
      for (let x = 0; x < this.width; x++) {
        const out = fn(x, y, this.width, this.height, this.rand);
        if (!out) continue;
        const i = (y * this.width + x) * 4;
        const d = this.data;
        const a = clamp(out[3] ?? 1);
        const dstA = d[i + 3] / 255;
        const outA = a + dstA * (1 - a);
        d[i] = Math.round((out[0] * a + d[i] * dstA * (1 - a)) / (outA || 1));
        d[i + 1] = Math.round((out[1] * a + d[i + 1] * dstA * (1 - a)) / (outA || 1));
        d[i + 2] = Math.round((out[2] * a + d[i + 2] * dstA * (1 - a)) / (outA || 1));
        d[i + 3] = Math.round(outA * 255);
      }
    }
    return this;
  }
  add(x, y, r, g, b) {
    if (x < 0 || y < 0 || x >= this.width || y >= this.height) return;
    const i = (y * this.width + x) * 4;
    this.data[i] = Math.min(255, this.data[i] + r);
    this.data[i + 1] = Math.min(255, this.data[i + 1] + g);
    this.data[i + 2] = Math.min(255, this.data[i + 2] + b);
  }
  toPNG() {
    return encodePNG(this.width, this.height, this.data);
  }
}

/**
 * Ghost silhouette in pixel space. Supersampled 3x3 so edges stay smooth at
 * every output size. cx/cy is the center, w/h the body size, wave the ruffle
 * depth as a fraction of the width.
 */
function ghostAlpha(px, py, cx, cy, w, h, wave = 0.07, step = 0.5) {
  const x = px - cx;
  const y = py - cy;
  const half = w / 2;
  const top = -h / 2;
  const bottom = sx => h / 2 - Math.cos((sx / half) * Math.PI * 1.5) * wave * w * 0.5;
  const inside = (sx, sy) => {
    if (sx < -half || sx > half) return false;
    if (sy < top + half) {
      const dx = sx;
      const dy = sy - (top + half);
      return dx * dx + dy * dy <= half * half;
    }
    return sy <= bottom(sx);
  };
  let hits = 0;
  for (let i = 0; i < 3; i++) {
    for (let j = 0; j < 3; j++) {
      if (inside(x + (i - 1) * step, y + (j - 1) * step)) hits++;
    }
  }
  return hits / 9;
}

function drawEyes(canvas, unit, cx, cy, scale, color, alpha = 0.92) {
  const eyeW = 0.052 * scale;
  const eyeH = 0.115 * scale;
  const gap = 0.115 * scale;
  const top = cy - 0.11 * scale;
  for (const dir of [-1, 1]) {
    const ex = cx + dir * gap;
    for (let y = Math.floor(top); y < top + eyeH * 2.2; y++) {
      for (let x = Math.floor(ex - eyeW); x < ex + eyeW; x++) {
        const dx = Math.abs(x - ex) / eyeW;
        const dy = Math.abs(y - (top + eyeH)) / eyeH;
        const d = Math.max(dx, dy);
        const a = 1 - smoothstep(0.75, 1.05, d);
        if (a > 0) canvas.blend(x, y, color[0], color[1], color[2], a * alpha);
      }
    }
  }
}

function drawHeadphones(canvas, unit, cx, cy, w, color) {
  const band = w * 0.62;
  const thickness = Math.max(2, w * 0.052);
  for (let y = Math.floor(cy - band); y < cy + band * 0.1; y++) {
    for (let x = Math.floor(cx - band); x < cx + band; x++) {
      const r = Math.hypot(x - cx, y - (cy + band * 0.35));
      const d = Math.abs(r - band * 0.92);
      const a = 1 - smoothstep(thickness * 0.5, thickness * 0.5 + 1.6, d);
      if (a > 0 && y < cy) canvas.blend(x, y, color[0], color[1], color[2], a * 0.95);
    }
  }
  for (const dir of [-1, 1]) {
    const ex = cx + dir * band * 0.92;
    const ey = cy + band * 0.18;
    const rx = thickness * 1.5;
    const ry = w * 0.115;
    for (let y = Math.floor(ey - ry * 1.6); y < ey + ry * 1.6; y++) {
      for (let x = Math.floor(ex - rx * 1.8); x < ex + rx * 1.8; x++) {
        const d = Math.hypot((x - ex) / rx, (y - ey) / ry);
        const a = 1 - smoothstep(0.82, 1.05, d);
        if (a > 0) canvas.blend(x, y, color[0], color[1], color[2], a);
      }
    }
  }
}

/* ------------------------------------------------------------- cover art --- */
const covers = {
  'after-hours': { top: '#25060f', bottom: '#0a0a0d', glow: '#ff3355', accent: '#ff6b7d', ghost: [255, 236, 238], rim: '#ff3355' },
  'cloud-nine': { top: '#111b38', bottom: '#080a12', glow: '#5f7dff', accent: '#9db4ff', ghost: [232, 240, 255], rim: '#6c8cff' },
  'neon-dreams': { top: '#22072e', bottom: '#08060f', glow: '#ff3fd8', accent: '#ff7ce8', ghost: [255, 232, 255], rim: '#ff3fd8' },
  'indie-spirit': { top: '#2a1707', bottom: '#0c0a08', glow: '#ff9a3c', accent: '#ffbe7a', ghost: [255, 244, 230], rim: '#ff9a3c' },
  'deep-focus': { top: '#04201f', bottom: '#070c0f', glow: '#20e0b4', accent: '#7cf3d6', ghost: [226, 255, 248], rim: '#20e0b4' },
  lofi: { top: '#1a1230', bottom: '#0a0810', glow: '#9b7bff', accent: '#c3b0ff', ghost: [241, 236, 255], rim: '#9b7bff' },
};

function paintCover(id, spec, size = 1024) {
  const canvas = new Canvas(size, size);
  const top = hex(spec.top);
  const bottom = hex(spec.bottom);
  const glow = hex(spec.glow);
  const accent = hex(spec.accent);
  const rim = hex(spec.rim);
  const unit = size;
  const cx = size * 0.5;
  const cy = size * 0.53;
  const ghostW = size * 0.42;
  const ghostH = size * 0.54;

  canvas.shade((x, y, w, h, rand) => {
    const v = y / h;
    const u = x / w;
    let r = mix(top[0], bottom[0], Math.pow(v, 0.85));
    let g = mix(top[1], bottom[1], Math.pow(v, 0.85));
    let b = mix(top[2], bottom[2], Math.pow(v, 0.85));

    // Soft key light behind the ghost.
    const d = Math.hypot(u - 0.5, (v - 0.46) * 1.05) / 0.62;
    const halo = Math.pow(clamp(1 - d), 3.1);
    r += glow[0] * halo * 0.55;
    g += glow[1] * halo * 0.55;
    b += glow[2] * halo * 0.55;

    // Diagonal sheen.
    const sheen = Math.pow(clamp(1 - Math.abs(u * 0.8 + v * 0.35 - 1.05)), 3) * 26;
    r += sheen; g += sheen; b += sheen * 1.1;

    // Vignette + grain keep the flat gradient from banding.
    const vig = 1 - 0.55 * Math.pow(Math.hypot(u - 0.5, v - 0.5) * 1.32, 2.4);
    const grain = (rand() - 0.5) * 7;
    return [clamp(r * vig + grain, 0, 255), clamp(g * vig + grain, 0, 255), clamp(b * vig + grain, 0, 255), 1];
  });

  // Glow pass behind the silhouette.
  glowPass(canvas, (x, y) => ghostAlpha(x, y, cx, cy, ghostW * 1.04, ghostH * 1.02), Math.round(size * 0.05), rim.map(c => c * 0.9), 0.85);

  // Ghost body with a soft top-down light.
  canvas.shade((x, y) => {
    const a = ghostAlpha(x, y, cx, cy, ghostW, ghostH);
    if (!a) return null;
    const t = clamp((y - (cy - ghostH / 2)) / ghostH);
    const light = 0.78 + 0.22 * (1 - t);
    const tintA = 0.10 + 0.22 * t;
    return [
      mix(spec.ghost[0], accent[0], tintA) * light,
      mix(spec.ghost[1], accent[1], tintA) * light,
      mix(spec.ghost[2], accent[2], tintA) * light,
      a * 0.94,
    ];
  });

  drawEyes(canvas, unit, cx, cy - ghostH * 0.02, size * 0.42, [16, 14, 20]);

  // Ground shadow.
  canvas.shade((x, y, w, h) => {
    const dx = (x - cx) / (ghostW * 0.62);
    const dy = (y - (cy + ghostH * 0.56)) / (size * 0.02);
    const d = dx * dx + dy * dy;
    if (d > 1) return null;
    return [0, 0, 0, (1 - d) * 0.45];
  });

  return canvas;
}

/**
 * Additive glow: rasterize a shape's coverage, soften it with two separable box
 * passes, then add it as light. Additive avoids the dark fringes a naive blur
 * of non-premultiplied RGBA would create.
 */
function glowPass(canvas, shape, radius, color, strength) {
  const { width, height } = canvas;
  let buffer = new Float32Array(width * height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) buffer[y * width + x] = shape(x, y);
  }
  const clampIndex = (v, max) => (v < 0 ? 0 : v > max ? max : v);
  let temp = new Float32Array(width * height);
  for (let pass = 0; pass < 2; pass++) {
    for (let y = 0; y < height; y++) {
      let sum = 0;
      for (let x = -radius; x <= radius; x++) sum += buffer[y * width + clampIndex(x, width - 1)];
      for (let x = 0; x < width; x++) {
        temp[y * width + x] = sum / (radius * 2 + 1);
        sum += buffer[y * width + clampIndex(x + radius + 1, width - 1)] - buffer[y * width + clampIndex(x - radius, width - 1)];
      }
    }
    for (let x = 0; x < width; x++) {
      let sum = 0;
      for (let y = -radius; y <= radius; y++) sum += temp[clampIndex(y, height - 1) * width + x];
      for (let y = 0; y < height; y++) {
        buffer[y * width + x] = sum / (radius * 2 + 1);
        sum += temp[clampIndex(y + radius + 1, height - 1) * width + x] - temp[clampIndex(y - radius, height - 1) * width + x];
      }
    }
  }
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const a = buffer[y * width + x] * strength;
      if (a > 0.002) canvas.add(x, y, color[0] * a, color[1] * a, color[2] * a);
    }
  }
}

/* --------------------------------------------------------------- hero art -- */
function paintHero(width = 1600, height = 1000) {
  const canvas = new Canvas(width, height);
  const deep = [10, 8, 12];
  const crimson = [120, 12, 34];
  const ember = [255, 74, 92];
  canvas.shade((x, y, w, h, rand) => {
    const u = x / w;
    const v = y / h;
    const d = Math.hypot((u - 0.62) * 1.05, (v - 0.55) * 1.2);
    const cloud = Math.pow(clamp(1 - d / 0.7), 2.4);
    let r = mix(deep[0], crimson[0], cloud * 0.95);
    let g = mix(deep[1], crimson[1], cloud * 0.95);
    let b = mix(deep[2], crimson[2], cloud * 0.95);
    const core = Math.pow(clamp(1 - d / 0.34), 3);
    r += ember[0] * core * 0.42;
    g += ember[1] * core * 0.42;
    b += ember[2] * core * 0.42;
    const vig = 1 - 0.5 * Math.pow(Math.hypot(u - 0.55, v - 0.5) * 1.3, 2.2);
    const grain = (rand() - 0.5) * 8;
    return [clamp(r * vig + grain, 0, 255), clamp(g * vig + grain, 0, 255), clamp(b * vig + grain, 0, 255), 1];
  });

  const unit = height;
  const cx = width * 0.635;
  const cy = height * 0.52;
  const ghostW = height * 0.36;
  const ghostH = height * 0.46;

  glowPass(canvas, (x, y) => ghostAlpha(x, y, cx, cy, ghostW * 1.1, ghostH * 1.06), 42, [255, 130, 150], 0.55);

  canvas.shade((x, y) => {
    const a = ghostAlpha(x, y, cx, cy, ghostW, ghostH);
    if (!a) return null;
    const t = clamp((y - (cy - ghostH / 2)) / ghostH);
    const light = 0.86 + 0.14 * (1 - t);
    return [246 * light, 238 * light, 244 * light, a * 0.96];
  });
  drawEyes(canvas, unit, cx, cy - ghostH * 0.01, height * 0.39, [22, 16, 24]);
  drawHeadphones(canvas, unit, cx, cy - ghostH * 0.2, ghostW * 1.05, [252, 244, 248]);

  // Floating notes around the silhouette.
  const noteColor = [255, 190, 200];
  const notes = [
    [cx - ghostW * 1.15, cy - ghostH * 0.5], [cx + ghostW * 1.2, cy - ghostH * 0.35],
    [cx + ghostW * 1.05, cy + ghostH * 0.45], [cx - ghostW * 1.0, cy + ghostH * 0.42],
  ];
  for (const [nx, ny] of notes) {
    for (let i = 0; i < 26; i++) {
      for (let j = 0; j < 26; j++) {
        const dx = (i - 13) / 13;
        const dy = (j - 13) / 13;
        if (dx * dx + dy * dy <= 1) canvas.blend(Math.round(nx + i), Math.round(ny + j), noteColor[0], noteColor[1], noteColor[2], 0.22 * (1 - Math.hypot(dx, dy)));
      }
    }
  }
  return canvas;
}

/* ----------------------------------------------------------------- icons --- */
function paintIcon(size, { maskable = false } = {}) {
  const canvas = new Canvas(size, size);
  const pad = maskable ? size * 0.17 : size * 0.06;
  canvas.shade((x, y, w, h) => {
    const u = x / w;
    const v = y / h;
    const radius = maskable ? 0.5 : 0.22;
    const dx = Math.max(Math.abs(u - 0.5) - (0.5 - radius), 0);
    const dy = Math.max(Math.abs(v - 0.5) - (0.5 - radius), 0);
    const dist = Math.hypot(dx, dy);
    const alpha = 1 - smoothstep(radius - 0.012, radius, dist);
    if (alpha <= 0) return null;
    const glow = Math.pow(clamp(1 - Math.hypot(u - 0.42, v - 0.38) * 1.9), 2.2);
    return [16 + glow * 62, 14 + glow * 12, 20 + glow * 42, alpha];
  });
  const ghostW = size - pad * 2.4;
  const ghostH = ghostW * 1.16;
  canvas.shade((x, y) => {
    const a = ghostAlpha(x, y, size / 2, size * 0.52, ghostW, ghostH);
    if (!a) return null;
    const t = clamp((y - (size * 0.52 - ghostH / 2)) / ghostH);
    const light = 0.8 + 0.2 * (1 - t);
    return [244 * light, 238 * light, 247 * light, a * 0.97];
  });
  drawEyes(canvas, size, size / 2, size * 0.5, ghostW * 1.0, [20, 16, 26]);
  return canvas;
}

/* ------------------------------------------------------------------ write -- */
mkdirSync('public/images', { recursive: true });
mkdirSync('public/icons', { recursive: true });
mkdirSync('assets', { recursive: true });

for (const [id, spec] of Object.entries(covers)) {
  writeFileSync(`public/images/${id}.png`, paintCover(id, spec).toPNG());
  console.log(`public/images/${id}.png`);
}
writeFileSync('public/images/ghost-hero.png', paintHero().toPNG());
console.log('public/images/ghost-hero.png');

/** Windows icon: ICO container with embedded PNG frames (Vista+). */
function encodeICO(frames) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(frames.length, 4);
  const entries = [];
  let offset = 6 + frames.length * 16;
  for (const { size, png } of frames) {
    const entry = Buffer.alloc(16);
    entry[0] = size >= 256 ? 0 : size;
    entry[1] = size >= 256 ? 0 : size;
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(png.length, 8);
    entry.writeUInt32LE(offset, 12);
    entries.push(entry);
    offset += png.length;
  }
  return Buffer.concat([header, ...entries, ...frames.map(frame => frame.png)]);
}

writeFileSync('assets/ghost-music.ico', encodeICO([16, 24, 32, 48, 64, 128, 256].map(size => ({ size, png: paintIcon(size).toPNG() }))));
console.log('assets/ghost-music.ico');

writeFileSync('public/icons/ghost-192.png', paintIcon(192, { maskable: true }).toPNG());
writeFileSync('public/icons/ghost-512.png', paintIcon(512, { maskable: true }).toPNG());
writeFileSync('public/icons/apple-touch-icon.png', paintIcon(180).toPNG());
writeFileSync('assets/icon-1024.png', paintIcon(1024).toPNG());
for (const [size, folder] of [[48, 'mipmap-mdpi'], [72, 'mipmap-hdpi'], [96, 'mipmap-xhdpi'], [144, 'mipmap-xxhdpi'], [192, 'mipmap-xxxhdpi']]) {
  writeFileSync(`assets/android-${folder}.png`, paintIcon(size).toPNG());
}
console.log('icons written');

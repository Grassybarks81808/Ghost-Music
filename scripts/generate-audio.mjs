import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync, unlinkSync, existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
const require = createRequire(import.meta.url);
const ffmpeg = require('ffmpeg-static');
const songs = [
 ['midnight-drive','Midnight Drive','After Hours',162,96,45,'after-hours'],
 ['weightless','Weightless','Head in the Clouds',188,76,50,'cloud-nine'],
 ['neon-dreams','Neon Dreams','Neon Nostalgia',204,112,42,'neon-dreams'],
 ['golden','Golden','Indie State of Mind',176,104,48,'indie-spirit'],
 ['slow-motion','Slow Motion','Deep Focus',192,68,41,'deep-focus'],
 ['somewhere-somehow','Somewhere, Somehow','Lowkey & Lo-fi',180,82,46,'lofi'],
 ['lunar-tides','Lunar Tides','After Hours',168,90,43,'after-hours'],
 ['blue-hour','Blue Hour','Deep Focus',196,72,47,'deep-focus'],
];
mkdirSync('public/audio', { recursive: true });
const sr = 22050;
const tau = Math.PI * 2;
const freq = midi => 440 * Math.pow(2, (midi - 69) / 12);
for (const [id,title,album,duration,bpm,root,cover] of songs) {
 if (existsSync(`public/audio/${id}.mp3`)) continue;
 const frames = duration * sr;
 const data = Buffer.alloc(44 + frames * 4);
 data.write('RIFF',0); data.writeUInt32LE(data.length-8,4); data.write('WAVEfmt ',8);
 data.writeUInt32LE(16,16); data.writeUInt16LE(1,20); data.writeUInt16LE(2,22);
 data.writeUInt32LE(sr,24); data.writeUInt32LE(sr*4,28); data.writeUInt16LE(4,32);
 data.writeUInt16LE(16,34); data.write('data',36); data.writeUInt32LE(frames*4,40);
 const beatLength = 60 / bpm;
 const progression = [0, 5, 3, 7];
 const arp = [12,19,15,22,19,15,10,19,12,15,22,24,19,15,10,7];
 let noise = 73829;
 for (let i=0;i<frames;i++) {
   const t = i/sr, beat=t/beatLength, bar=Math.floor(beat/4);
   const chord = root + progression[Math.floor(bar/2)%4];
   const bt = (beat % 1) * beatLength;
   const fade = Math.min(1,t/3,(duration-t)/5);
   const padEnv = .45 + .15*Math.sin(t*.6);
   const pad = (Math.sin(tau*freq(chord+12)*t)*.07 + Math.sin(tau*freq(chord+15)*t)*.045 + Math.sin(tau*freq(chord+19)*t + .5*Math.sin(t*.2))*.04)*padEnv;
   const kick = Math.sin(tau*(47*bt+8*(1-Math.exp(-bt*28))))*Math.exp(-bt*14)*.3;
   const bass = Math.sin(tau*freq(chord-12)*t)*.16*Math.exp(-bt*2.5);
   noise = (noise * 16807) % 2147483647;
   const n = noise/1073741823.5-1;
   const hatTime = (beat%.5)*beatLength;
   const hat = n*Math.exp(-hatTime*100)*.038;
   const snare = Math.floor(beat)%2 ? n*Math.exp(-bt*28)*.08 : 0;
   const mstep = Math.floor(beat*2);
   const mt = (beat*2%1)*beatLength/2;
   const melodyFrequency = freq(chord + arp[mstep%arp.length]);
   const melody = (Math.sin(tau*melodyFrequency*t)+.16*Math.sin(tau*melodyFrequency*2*t))*.09*Math.exp(-mt*6)*Math.min(1,mt*150);
   const variation = bar%8<6 ? 1 : .6;
   const sample = (pad+bass+kick*variation+hat+snare*variation+melody)*fade;
   const stereo = Math.sin(tau*freq(chord+19)*t+.1)*.012*fade;
   data.writeInt16LE(Math.round(Math.tanh(sample+stereo)*26000),44+i*4);
   data.writeInt16LE(Math.round(Math.tanh(sample-stereo)*26000),46+i*4);
 }
 const temp = `public/audio/${id}.wav`;
 writeFileSync(temp,data);
 const args=['-y','-hide_banner','-loglevel','error','-i',temp,'-i',`public/images/${cover}.png`,'-map','0:a','-map','1:v','-c:a','libmp3lame','-b:a','160k','-c:v','mjpeg','-vf','scale=400:400','-id3v2_version','3','-metadata',`title=${title}`,'-metadata','artist=Ghost Sessions','-metadata',`album=${album}`,'-metadata','comment=Original synthesized instrumental created for Ghost Music. Free to listen and download.','-metadata:s:v','title=Album cover','-metadata:s:v','comment=Cover (front)',`public/audio/${id}.mp3`];
 const result=spawnSync(ffmpeg,args,{stdio:'inherit'});
 if(result.status!==0) throw new Error(`Could not encode ${id}`);
 unlinkSync(temp);
 console.log(`Created ${title} (${duration}s) with embedded artwork and metadata`);
}

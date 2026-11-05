// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
const sources=JSON.parse(readFileSync('branding/source-hashes.json','utf8'));
for(const [name,hash] of Object.entries(sources))if(createHash('sha256').update(readFileSync(`branding/${name}`)).digest('hex')!==hash)throw new Error(`Approved artwork changed: ${name}`);
const sizes=[16,24,32,48,64,128,256,512,1024];
const result=spawnSync(process.execPath,['node_modules/@tauri-apps/cli/tauri.js','icon','branding/packit-icon.png','--output','src-tauri/icons/windows',...sizes.flatMap(n=>['-p',String(n)])],{stdio:'inherit'});
if(result.status!==0)process.exit(result.status??1);
const frames=sizes.filter(n=>n<=256).map(n=>({n,bytes:readFileSync(`src-tauri/icons/windows/${n}x${n}.png`)}));
const head=Buffer.alloc(6);head.writeUInt16LE(1,2);head.writeUInt16LE(frames.length,4);let offset=6+16*frames.length;
const directory=frames.map(({n,bytes})=>{const b=Buffer.alloc(16);b[0]=b[1]=n===256?0:n;b.writeUInt16LE(1,4);b.writeUInt16LE(32,6);b.writeUInt32LE(bytes.length,8);b.writeUInt32LE(offset,12);offset+=bytes.length;return b;});
writeFileSync('src-tauri/icons/icon.ico',Buffer.concat([head,...directory,...frames.map(f=>f.bytes)]));
writeFileSync('src-tauri/icons/icon.png',readFileSync('src-tauri/icons/windows/256x256.png'));
const macFrames=[["ic07",128],["ic08",256],["ic09",512],["ic10",1024]].map(([code,n])=>{const bytes=readFileSync(`src-tauri/icons/windows/${n}x${n}.png`);const b=Buffer.alloc(8);b.write(code);b.writeUInt32BE(8+bytes.length,4);return Buffer.concat([b,bytes]);});
const macHead=Buffer.alloc(8);macHead.write('icns');macHead.writeUInt32BE(8+macFrames.reduce((n,b)=>n+b.length,0),4);writeFileSync('src-tauri/icons/icon.icns',Buffer.concat([macHead,...macFrames]));
writeFileSync('public/packit-favicon.png',readFileSync('src-tauri/icons/windows/32x32.png'));

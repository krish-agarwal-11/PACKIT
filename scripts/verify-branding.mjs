// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import assert from 'node:assert/strict';import{readFileSync}from'node:fs';import{createHash}from'node:crypto';
const sources=JSON.parse(readFileSync('branding/source-hashes.json','utf8'));
for(const[name,hash]of Object.entries(sources))assert.equal(createHash('sha256').update(readFileSync(`branding/${name}`)).digest('hex'),hash);
const ico=readFileSync('src-tauri/icons/icon.ico');assert.equal(ico.readUInt16LE(2),1);assert.equal(ico.readUInt16LE(4),7);
for(const[index,size]of[16,24,32,48,64,128,256].entries()){const at=6+index*16;assert.equal(ico[at]||256,size);assert.equal(ico[at+1]||256,size);const n=ico.readUInt32LE(at+8);const pos=ico.readUInt32LE(at+12);assert.ok(pos+n<=ico.length);const png=ico.subarray(pos,pos+n);assert.equal(png.readUInt32BE(16),size);assert.equal(png.readUInt32BE(20),size);assert.deepEqual(png,readFileSync(`src-tauri/icons/windows/${size}x${size}.png`));}
assert.deepEqual(readFileSync('src-tauri/icons/icon.png'),readFileSync('src-tauri/icons/windows/256x256.png'));
console.log('Branding verified: original hashes unchanged; seven ICO resolutions and PNG assets valid.');

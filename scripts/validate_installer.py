# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Validate private NSIS build inputs without installing or changing registrations."""
import hashlib,json,re,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
load=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
def pe(path):
 data=path.read_bytes();assert data[:2]==b'MZ';offset=struct.unpack_from('<I',data,0x3c)[0];assert data[offset:offset+4]==b'PE\0\0';machine,count=struct.unpack_from('<HH',data,offset+4);optional=offset+24;size=struct.unpack_from('<H',data,offset+20)[0];magic=struct.unpack_from('<H',data,optional)[0];directories=optional+(112 if magic==0x20b else 96);sections=[]
 for i in range(count):
  start=optional+size+40*i;virtual,base,raw,position=struct.unpack_from('<IIII',data,start+8);sections.append((base,max(virtual,raw),position))
 def at(rva):
  for base,length,position in sections:
   if base<=rva<base+length:return position+rva-base
  raise ValueError('PE RVA outside sections')
 imports=[];rva=struct.unpack_from('<I',data,directories+8)[0]
 if rva:
  start=at(rva)
  for i in range(512):
   record=data[start+20*i:start+20*i+20]
   if record==bytes(20):break
   name=at(struct.unpack_from('<I',record,12)[0]);imports.append(data[name:data.index(b'\0',name)].decode('ascii'))
 return {'machine':hex(machine),'imports':imports,'sha256':sha(path),'bytes':len(data)}
def main():
 config=load(ROOT/'src-tauri/tauri.conf.json');version=config['version'];assert version==load(ROOT/'package.json')['version']==load(ROOT/'package-lock.json')['version']=='0.1.0'
 platform=load(ROOT/'src-tauri/tauri.windows.conf.json');assert 'resources' not in platform['bundle'],'Platform override must not replace the explicit legal/resource map'
 b=config['bundle'];assert config['identifier']=='org.packit.desktop' and b['publisher']=='Krish Agarwal' and b['targets']==['nsis'];assert b['windows']['nsis']['installMode']=='currentUser';assert not b['windows']['allowDowngrades'] and b['windows']['webviewInstallMode']=={'type':'skip'}
 assert config['plugins']['updater']['requireSignedVersion'] is True and not b.get('createUpdaterArtifacts',False)
 assert load(ROOT/'src-tauri/update-policy.json')['feedReady'] is False
 assert config['plugins']['updater']['pubkey']==(ROOT/'src-tauri/updater-public-key.pub').read_text().strip()
 imports={};manifest=load(ROOT/'src-tauri/vendor/shell/manifest.json')
 for name,path in [('app',ROOT/'src-tauri/target/x86_64-pc-windows-msvc/release/packit.exe'),('shell',ROOT/'src-tauri/vendor/shell'/manifest['filename']),('7z.exe',ROOT/'src-tauri/vendor/7zip/7z.exe'),('7z.dll',ROOT/'src-tauri/vendor/7zip/7z.dll')]:
  imports[name]=pe(path);assert imports[name]['machine']=='0x8664';assert not any(re.match(r'(vcruntime|msvcp|msvcr)\d',d,re.I) for d in imports[name]['imports']),(name,imports[name]['imports'])
 assert imports['shell']['sha256']==manifest['sha256']
 for item in load(ROOT/'legal/7zip-release-provenance.json')['binary_comparison']['files']:assert sha(ROOT/'src-tauri/vendor/7zip'/item['name'])==item['sha256']
 hooks=(ROOT/'src-tauri/windows/installer-hooks.nsh').read_text();assert '--shell-register' in hooks and '--shell-unregister' in hooks and '--dev' not in hooks;assert '!macroundef CheckIfAppIsRunning' in hooks and 'RmShutdown' not in hooks
 assert '${If} $0 < 18362' in hooks and '${If} $0 < 22000' not in hooks
 assert 'IsWow64Process2' in hooks and '$1 != 34404' in hooks and 'Microsoft\\EdgeUpdate' in hooks
 assert 'SetOverwrite off' in hooks and '--installer-check' in hooks and '--installer-clean-shell' in hooks
 assert b['licenseFile']=='../LICENSE' and (ROOT/'LICENSE').stat().st_size==35149
 installer=ROOT/f'src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/PACKIT_{version}_x64-setup.exe';assert installer.is_file();installer_pe=pe(installer)
 for file in ['LICENSE','THIRD_PARTY_NOTICES.md','legal/SOURCE_DISTRIBUTION.md','legal/RELEASE_REVIEW.md']:assert (ROOT/file).is_file()
 packaged=ROOT/'experiments/license-audit/stage4/packit-packaged.exe'
 if packaged.exists():imports['packaged_app']=pe(packaged)
 output=ROOT/'experiments/license-audit/stage4';output.mkdir(parents=True,exist_ok=True);report={'version':version,'target':'x86_64-pc-windows-msvc','scope':'currentUser','installer':{'file':installer.relative_to(ROOT).as_posix(),**installer_pe},'binaries':imports,'manual_installation':'NOT performed; Windows 10/11 manual acceptance required','legal_clearance':False};(output/'installer-validation.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
if __name__=='__main__':main()

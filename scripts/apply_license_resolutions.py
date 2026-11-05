# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Apply reviewed supplemental evidence without rerunning the Stage 3 audit."""
from pathlib import Path
import base64,hashlib,json,tarfile,zipfile
ROOT=Path(__file__).resolve().parent.parent
L=ROOT/'legal';S=ROOT/'experiments/license-audit/stage3b'
def sha(b):return hashlib.sha256(b).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
inv=read(L/'inventory.json');evidence=read(L/'evidence-manifest.json');extra=read(L/'recovery-downloads.json')['requests'];meta=read(ROOT/'experiments/license-audit/cargo-main.json')
cache={p['name']:Path(p['manifest_path']).parent for p in meta['packages']}
notices={e['file']:e for e in evidence['files']}
for r in extra:
 if r.get('notice'):
  p=ROOT/r['notice'];notices[r['notice']]={'file':r['notice'],'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size,'full_text_candidate':not p.name.endswith('.spdx'),'retrieval':{k:v for k,v in r.items() if k not in ['local','notice','notice_sha256']}}
def attach(row,filter):
 row['notices'] += [e for k,e in notices.items() if filter(k) and k not in {n['file'] for n in row['notices']}]
 row['compatibility_review']='RESOLVED — exact-version upstream permission/notice evidence recovered; preserve supplied texts. Separate native/source questions are tracked independently.'
for row in inv['records']:
 name=row['name']
 if row['ecosystem']=='cargo' and name=='alloc-stdlib':attach(row,lambda k:'github-dropbox__rust-alloc-no-stdlib-0a81fd' in k)
 if row['ecosystem']=='cargo' and name=='tauri-plugin':attach(row,lambda k:'github-tauri-apps__tauri-59b63a' in k)
 if row['ecosystem']=='cargo' and name in ['webview2-com','webview2-com-sys','webview2-com-macros']:
  commit=row['evidence']['vcs']['git']['sha1'];attach(row,lambda k:('github-wravery__webview2-rs-'+commit) in k)
 if row['ecosystem']=='npm' and name=='@tauri-apps/plugin-dialog':attach(row,lambda k:'npm-tree-@tauri-apps__plugin-dialog-2.8.4__LICENSE' in k)
 if row['ecosystem']=='npm' and row['distribution']=='build/development only' and row['compatibility_review'].startswith('UNRESOLVED'):
  row['compatibility_review']='RESOLVED scope — unmodified build tool not conveyed in the defined binary/source payload. Not a runtime blocker; re-audit if this tool itself is bundled.'
# Check all shipped MPL source packages, not only the original missing-file candidate.
mpl=[]
for row in inv['records']:
 if row['ecosystem']!='cargo' or row['declared_license']!='MPL-2.0' or not row['windows_uses']:continue
 name=row['name'];version=row['version'];d=cache[name];archive=d.parent.parent.parent/'cache'/d.parent.name/f'{name}-{version}.crate';files=[];markers=[];changed=[]
 with tarfile.open(archive) as t:
  for m in t.getmembers():
   if not m.isfile():continue
   rel=Path(m.name).relative_to(f'{name}-{version}');data=t.extractfile(m).read();local=d/rel
   if not local.exists() or local.read_bytes()!=data:changed.append(str(rel))
   if rel.suffix in ['.rs','.c','.h','.cpp','.py']:
    text=data.decode('utf-8',errors='replace');files.append({'file':rel.as_posix(),'sha256':sha(data),'mpl_header':('Mozilla Public' in text[:1500] or 'MPL-2.0' in text[:1500])})
    if 'Incompatible With Secondary Licenses' in text or 'Incompatible with Secondary Licenses' in text:markers.append(rel.as_posix())
 assert not changed,(name,changed)
 out=L/'sources/mpl'/archive.name;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(archive.read_bytes())
 entry={'name':name,'version':version,'source':out.relative_to(ROOT).as_posix(),'sha256':sha(out.read_bytes()),'source_files':files,'exhibit_b_source_markers':markers,'local_modifications':changed,'mechanism':'MPL 2.0 section 3.3 unmodified files in GPL-3.0-only Larger Work; preserve MPL source terms/headers'};mpl.append(entry)
 if name=='selectors':
  header=d/'lib.rs';out=L/f'upstream/recovered/selectors-{version}/lib.rs';out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(header.read_bytes());e={'file':out.relative_to(ROOT).as_posix(),'sha256':sha(out.read_bytes()),'bytes':out.stat().st_size,'full_text_candidate':False,'evidence_type':'exact crate source with Exhibit A notice; full text legal/licenses/MPL-2.0.txt','registry_archive_bytes_match':True};notices[e['file']]=e;row['notices'].append(e)
 row['compatibility_review']='RESOLVED notice/eligibility review — checked unmodified source, no applied Exhibit B found; MPL text/source preserved. PREPARED for final paired-source delivery under section 3.3.'
write(L/'mpl-source-review.json',{'components':mpl,'source_delivery':'included locally in legal/sources/mpl; final binary/source pairing and delivery verification pending'})
# Native loader provenance now tied to exact NuGet SDK content.
with zipfile.ZipFile(S/'webview2-sdk-1.0.3800.47.nupkg') as z:
 d=cache['webview2-com-sys'];matches=[]
 for arch in ['x64','x86','arm64']:
  for name in ['WebView2Loader.dll','WebView2Loader.dll.lib','WebView2LoaderStatic.lib']:
   actual=(d/arch/name).read_bytes();official=z.read(f'build/native/{arch}/{name}');assert actual==official,(arch,name);matches.append({'arch':arch,'file':name,'sha256':sha(actual),'matches_sdk':True})
 native=read(L/'NATIVE_COMPONENTS.json');native['sdk_provenance']={'package':'Microsoft.Web.WebView2','version':'1.0.3800.47','url':'https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/1.0.3800.47/microsoft.web.webview2.1.0.3800.47.nupkg','sha256':sha((S/'webview2-sdk-1.0.3800.47.nupkg').read_bytes()),'license':'BSD-3-Clause text with Microsoft copyright; entire SDK NOTICE retained','matches':matches,'remaining':'GPL Corresponding Source / System Library treatment of static loader; SDK package contains no loader implementation source'};write(L/'NATIVE_COMPONENTS.json',native)
# Retire removed assets in the existing inventory, retain original provenance record separately.
for asset in inv['assets']:
 if not (ROOT/asset['file']).exists():asset['distribution']='REMOVED — unused asset, excluded from payload/source';asset['excluded']=True
# Refresh only root lock metadata; do not promote the actual app or resolve dependencies.
pkg=read(ROOT/'package.json');lock=read(ROOT/'package-lock.json');before={k:v.get('version') for k,v in lock['packages'].items() if k};lock['version']=pkg['version'];lock['packages']['']['version']=pkg['version'];assert before=={k:v.get('version') for k,v in lock['packages'].items() if k};write(ROOT/'package-lock.json',lock)
inv['inputs']['package-lock.json']=sha((ROOT/'package-lock.json').read_bytes());inv['npm_root_version_drift']={'package':pkg['version'],'lock':lock['version'],'lock_root':lock['packages']['']['version'],'resolution':'aligned stale root lock metadata to existing application version; no dependency/app version promotion'}
inv['stage3b_resolutions']='legal/RELEASE_REVIEW.md';write(L/'inventory.json',inv);write(L/'evidence-manifest.json',{'files':[notices[k] for k in sorted(notices)]})
print('Applied version-pinned notices, preserved MPL sources and nine matching SDK loader artifacts; retired unused assets; aligned lock root metadata.')

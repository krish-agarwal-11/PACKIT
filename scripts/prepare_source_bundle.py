# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Create an internal, deterministic Windows source candidate. Never grants release clearance."""
import argparse,gzip,hashlib,io,json,os,stat,tarfile,tomllib,urllib.request
from pathlib import Path,PurePosixPath
ROOT=Path(__file__).resolve().parent.parent
SCRATCH=ROOT/'experiments/license-audit'
def sha(data):return hashlib.sha256(data).hexdigest()
def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def safe(name):
 p=PurePosixPath(name)
 if p.is_absolute() or '..' in p.parts or ':' in name or '\\' in name:raise ValueError('Unsafe source member: '+name)
 return p.as_posix()
def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',default='experiments/license-audit/source-candidate.tar.gz');parser.add_argument('--build-record',help='Optional final build record, checked but not legal approval');args=parser.parse_args()
 output=(ROOT/args.output).resolve();assert any(output.is_relative_to(base.resolve()) for base in [SCRATCH,ROOT/'release-staging']),'Use an ignored audit/release-staging directory; distribution is not cleared'
 inv=load(ROOT/'legal/inventory.json');assert all(sha((ROOT/name).read_bytes())==digest for name,digest in inv['inputs'].items()),'Inventory stale: recheck changed locks/manifests before packaging';exclusions=load(ROOT/'legal/distribution-exclusions.json');excluded={e['file'] for e in exclusions['local_only_fixtures']+exclusions['removed_assets']};files={};origins={}
 def add(name,data,origin):
  name=safe(name)
  if name in files and files[name]!=data:raise ValueError('Source collision: '+name)
  files[name]=data;origins[name]=origin
 def addfile(path,name=None):
  if path.is_symlink() or path.stat().st_file_attributes & getattr(stat,'FILE_ATTRIBUTE_REPARSE_POINT',0):raise ValueError('Reparse point in source: '+str(path))
  add(name or path.relative_to(ROOT).as_posix(),path.read_bytes(),'workspace')
 prefixes=['src','scripts','legal','branding','public','docs','release','src-tauri','windows-shell']
 for prefix in prefixes:
  for base,dirs,names in os.walk(ROOT/prefix):
   dirs[:]=sorted(d for d in dirs if d not in ['target','gen','node_modules','__pycache__','.git'])
   for name in sorted(names):
    p=Path(base)/name;rel=p.relative_to(ROOT).as_posix()
    if rel not in excluded and not p.suffix.lower() in ['.pdb','.log','.pyc']:addfile(p)
 for p in ROOT.iterdir():
  if p.is_file() and (p.suffix in ['.json','.ts','.html','.md'] or p.name in ['LICENSE','.gitignore','.gitattributes','.oxlintrc.json']):addfile(p)
 addfile(ROOT/'.cargo/config.toml')
 project_files=dict(files);tree=sha(''.join(n+'\0'+sha(project_files[n])+'\n' for n in sorted(project_files)).encode())
 metas=[load(p) for p in [SCRATCH/'cargo-main.json',SCRATCH/'cargo-shell.json'] if p.exists()];packages={(p['name'],p['version']):p for meta in metas for p in meta['packages']};count=0
 for row in inv['records']:
  if row['ecosystem']!='cargo' or not row.get('windows_uses'):continue
  p=packages.get((row['name'],row['version']));candidates=[]
  if p:
   directory=Path(p['manifest_path']).parent;candidates.append(directory.parent.parent.parent/'cache'/directory.parent.name/f"{row['name']}-{row['version']}.crate")
  cargo_home=Path(os.environ.get('CARGO_HOME',str(Path.home()/'.cargo')));candidates+=list((cargo_home/'registry/cache').glob(f"*/{row['name']}-{row['version']}.crate"))
  archive=next((a for a in candidates if a.exists()),None)
  if archive is None:raise SystemExit('Missing locked crate archive: '+row['name']+' '+row['version']+'; run cargo fetch --locked for both manifests with the original registry configuration')
  raw=archive.read_bytes()
  checksums={x['name']+'@'+x['version']:x.get('checksum') for lock in ['src-tauri/Cargo.lock','windows-shell/Cargo.lock'] for x in tomllib.loads((ROOT/lock).read_text())['package']};assert sha(raw)==checksums[row['name']+'@'+row['version']]
  hashes={};prefix=row['name']+'-'+row['version']
  with tarfile.open(archive) as bundle:
   for member in bundle:
    if member.isdir():continue
    if not member.isfile():raise ValueError('Unexpected Cargo archive entry')
    rel=PurePosixPath(member.name).relative_to(prefix).as_posix();data=bundle.extractfile(member).read();add('vendor/cargo/'+prefix+'/'+rel,data,'locked crates.io archive');hashes[rel]=sha(data)
  add('vendor/cargo/'+prefix+'/.cargo-checksum.json',json.dumps({'files':hashes,'package':sha(raw)},sort_keys=True).encode(),'generated Cargo directory-source verification');count+=1
 files['.cargo/config.toml'] += b'\n[source.crates-io]\nreplace-with = "packit-vendored"\n[source.packit-vendored]\ndirectory = "vendor/cargo"\n';origins['.cargo/config.toml']='project CRT settings plus generated source-package registry configuration'
 def recovered(path,digest,url):
  if not path.exists():
   with urllib.request.urlopen(url,timeout=30) as response:data=response.read((64<<20)+1)
   assert len(data)<=64<<20 and sha(data)==digest,'Source recovery hash mismatch'
   path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
  assert sha(path.read_bytes())==digest
 npm=load(ROOT/'legal/frontend-source-provenance.json');npm_lock=load(ROOT/'package-lock.json')
 for component in npm['components']:
  p=ROOT/component['archive'];recovered(p,component['sha256'],npm_lock['packages']['node_modules/'+component['name']]['resolved']);addfile(p,'vendor/npm/'+component['name'].replace('/','__')+'-'+component['version']+'.tgz')
 for request in npm['requests']:
  assert request['status']=='retrieved',request
  p=ROOT/request['local'];recovered(p,request['sha256'],request['url']);addfile(p,'vendor/preferred/'+request['id']+'.tar.gz')
 # Tailwind's preferred sources/build inputs are in its exact installed package.
 for row in inv['records']:
  if row['ecosystem']=='npm' and row['name']=='tailwindcss':
   package=ROOT/'node_modules/tailwindcss';assert (package/'preflight.css').is_file(),'Run locked npm ci to recover emitted CSS source inputs'
   for p in package.rglob('*'):
    if p.is_file() and (p.suffix=='.css' or p.name in ['package.json','LICENSE']):addfile(p,'vendor/css/tailwindcss/'+p.relative_to(package).as_posix())
 upstream=ROOT/'legal/sources/7zip/7z2605-src.tar.xz';addfile(upstream,'vendor/upstream/7z2605-src.tar.xz')
 note=('INTERNAL SOURCE CANDIDATE — NOT RELEASE CLEARED\nWindows x64; run npm ci (locked unmodified build tools fetched separately), npm run build, then cargo build --locked --manifest-path src-tauri/Cargo.toml and windows-shell/Cargo.toml with the documented MSVC/Windows SDK toolchain.\nCargo Windows selected sources are vendored. Other target graphs are not certified offline.\nRuntime npm archives and preferred upstream trees are included under vendor/. node_modules build-tool executables are deliberately not conveyed.\nUncertain RAR fixtures remain local-only; scripts/restore_test_fixtures.py describes optional retrieval before tests.\nPrivate installer scripts/build-record pairing are included when supplied. Final public build configuration, clean-room reconstruction, source delivery and the two external legal questions remain pending.\n')
 add('SOURCE-BUILD-NOTES.txt',note.encode(),'generated build notes')
 build=None
 if args.build_record:
  build=load(ROOT/args.build_record)
  assert build['target']=='x86_64-pc-windows-msvc' and build['source_tree_sha256']==tree
  assert all(build.get(k) for k in ['profile','features','toolchain','artifacts'])
  for artifact in build['artifacts']:
   p=(ROOT/artifact['file']).resolve();assert p.is_relative_to(ROOT) and sha(p.read_bytes())==artifact['sha256']
 manifest={'status':'PREPARED internal candidate; NOT legal approval or fulfilled source delivery','target':'x86_64-pc-windows-msvc','source_tree_sha256':tree,'build_record':build,'cargo_source_packages':count,'excluded_fixtures':exclusions['local_only_fixtures'],'pending':['7-Zip/unRAR aggregation review','WebView2 static-loader Corresponding Source / System Library review','final public installer configuration and recipient reconstruction','recipient-facing delivery and clean reconstruction'],'files':[{'file':n,'sha256':sha(files[n]),'bytes':len(files[n]),'origin':origins[n]} for n in sorted(files)]}
 raw=json.dumps(manifest,ensure_ascii=False,indent=2).encode();add('SOURCE-MANIFEST.json',raw,'generated manifest');output.parent.mkdir(parents=True,exist_ok=True)
 with output.open('wb') as sink,gzip.GzipFile(filename='',mode='wb',fileobj=sink,mtime=0) as compressed,tarfile.open(fileobj=compressed,mode='w|',format=tarfile.PAX_FORMAT) as bundle:
  for name in sorted(files):
   data=files[name];info=tarfile.TarInfo(name);info.size=len(data);info.mode=0o644;info.mtime=0;bundle.addfile(info,io.BytesIO(data))
 digest=sha(output.read_bytes());output.with_suffix(output.suffix+'.sha256').write_text(digest+'  '+output.name+'\n');(SCRATCH/'source-candidate-manifest.json').write_bytes(raw)
 print(json.dumps({'artifact':output.relative_to(ROOT).as_posix(),'sha256':digest,'files':len(files),'cargo_packages':count,'source_tree_sha256':tree,'status':'INTERNAL PREPARED; final build/delivery/legal review pending'},indent=2))
if __name__=='__main__':main()

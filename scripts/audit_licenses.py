# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Evidence inventory, not a legal clearance scanner. Standard library only."""
import argparse, base64, collections, hashlib, json, os, re, shutil, subprocess, sys, tarfile, tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
LEGAL=ROOT/'legal'
def sha(data): return hashlib.sha256(data).hexdigest()
def load(path): return json.loads(Path(path).read_text(encoding='utf-8-sig'))
def write(path,value):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(value,encoding='utf-8',newline='\n')
def license_files(directory):
    result=[]
    for base,dirs,files in os.walk(directory):
        dirs[:]=[d for d in dirs if d not in ['node_modules','.git','target']]
        for name in files:
            low=name.lower()
            if any(low.startswith(k) for k in ['license','licence','copying','notice','copyright','unlicense']) and not low.endswith(('.rs','.js','.ts','.html','.json','.mjs','.cjs','.map','.c','.h','.svg','.png','.css')):
                result.append(Path(base)/name)
    return sorted(result)
def copy_evidence(path,base,dest):
    rel=path.relative_to(base).as_posix();data=path.read_bytes();target=LEGAL/'upstream'/dest/rel
    target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
    return {'source_relative_path':rel,'file':target.relative_to(ROOT).as_posix(),'sha256':sha(data),'bytes':len(data),'full_text_candidate':not rel.lower().endswith('.spdx') and len(data)>150}
def obligations(license):
    if not license:return 'UNRESOLVED: license declaration missing; obtain upstream permission and notices.'
    items=['Retain applicable copyright, permission, disclaimer and NOTICE files; do not replace upstream terms with PACKIT GPL.']
    if 'MPL' in license:items+=['Provide MPL-covered source and modifications under MPL; review secondary-license eligibility/GPL larger-work conditions.']
    if 'Apache' in license:items+=['If relying on Apache-2.0, retain license/NOTICE and modification notices; respect patent/attribution clauses.']
    if 'Unicode' in license:items+=['Retain Unicode/data copyright and permission notices, including licenses of generated tables.']
    if 'GPL' in license:items+=['Provide corresponding source and applicable GPL/LGPL rights; examine exact linkage and exceptions.']
    return ' '.join(items)
def review(license,name,coverage):
    if name.startswith('webview2-com'):return 'UNRESOLVED / RELEASE BLOCKER: wrapper declaration does not clear native Microsoft loader terms; exact wrapper notices also need coverage.'
    if not coverage:return 'UNRESOLVED / RELEASE BLOCKER: exact upstream license/attribution text missing locally.'
    if license and 'MPL' in license:return 'UNRESOLVED / RELEASE BLOCKER: MPL source delivery and GPL secondary-license/larger-work review required.'
    return 'VERIFIED: local declaration and notice evidence; compatibility is a package-level assessment, not legal certification.'
def runtime_graph(meta):
    packages={p['id']:p for p in meta['packages']};nodes={n['id']:n for n in meta['resolve']['nodes']};states=collections.defaultdict(set)
    todo=[(meta['resolve']['root'],'runtime')]
    while todo:
        id,mode=todo.pop()
        if mode in states[id]:continue
        states[id].add(mode)
        for dep in nodes.get(id,{}).get('deps',[]):
            proc=any('proc-macro' in t['kind'] for t in packages[dep['pkg']]['targets'])
            for k in dep['dep_kinds']:
                kind=k['kind'];new='test' if kind=='dev' else 'build' if kind=='build' or proc else mode
                if mode!='runtime' and kind!='dev':new=mode
                todo.append((dep['pkg'],new))
    return packages,states
def generate():
    records=[];selected={};cache_roots=set();lock_versions=collections.defaultdict(set);inputs={}; lock_checksums={}
    old_evidence=load(LEGAL/'evidence-manifest.json')['files'] if (LEGAL/'evidence-manifest.json').exists() else []
    scratch=ROOT/'experiments/license-audit';scratch.mkdir(parents=True,exist_ok=True)
    for label,manifest in [('main','src-tauri/Cargo.toml'),('shell','windows-shell/Cargo.toml')]:
        raw=subprocess.check_output(['cargo','metadata','--manifest-path',str(ROOT/manifest),'--locked','--offline','--filter-platform','x86_64-pc-windows-msvc','--format-version','1'],cwd=ROOT)
        meta=json.loads(raw);packages,states=runtime_graph(meta)
        (scratch/f'cargo-{label}.json').write_bytes(raw)
        for p in packages.values():
            if p.get('source'):
                cache_roots.add(Path(p['manifest_path']).parent.parent)
                selected.setdefault((p['name'],p['version']),{})[label]=sorted(states[p['id']])
        lock=manifest.replace('Cargo.toml','Cargo.lock');inputs[lock]=sha((ROOT/lock).read_bytes())
        for p in tomllib.loads((ROOT/lock).read_text())['package']:
            lock_versions[(p['name'],p['version'])].add(label)
            lock_checksums[(p['name'],p['version'])]=p.get('checksum')
    for (name,version),locks in sorted(lock_versions.items()):
        if name in ['packit','packit-shell']:continue
        directory=next((r/f'{name}-{version}' for r in cache_roots if (r/f'{name}-{version}/Cargo.toml').exists()),None)
        evidence=[];data={};vcs={}
        if directory:
            manifest=directory/'Cargo.toml';data=tomllib.loads(manifest.read_text())['package']
            if (directory/'.cargo_vcs_info.json').exists():vcs=load(directory/'.cargo_vcs_info.json')
            if (name,version) in selected:
                evidence=[copy_evidence(p,directory,Path('cargo')/f'{name}-{version}') for p in license_files(directory)]
            manifest_hash=sha(manifest.read_bytes())
        else:manifest_hash=None
        archive_check={'lock_checksum':lock_checksums.get((name,version)),'available':False}
        if directory:
            cached=directory.parent.parent.parent/'cache'/directory.parent.name/f'{name}-{version}.crate'
            if cached.exists():
                archive_check.update(available=True,sha256=sha(cached.read_bytes()))
                archive_check['matches_lock']=archive_check['sha256']==archive_check['lock_checksum']
                with tarfile.open(cached) as bundle:
                    for e in evidence:
                        try:e['registry_archive_bytes_match']=bundle.extractfile(f'{name}-{version}/{e["source_relative_path"]}').read()==(ROOT/e['file']).read_bytes()
                        except (KeyError,AttributeError):e['registry_archive_bytes_match']=False
        license=data.get('license');coverage=any(e['full_text_candidate'] for e in evidence)
        uses=selected.get((name,version),{});modes=set(m for modes in uses.values() for m in modes)
        delivery='compiled Windows candidate (conservative normal-dependency graph)' if 'runtime' in modes else 'build/proc-macro/generated-output review' if 'build' in modes else 'test/development only' if modes else 'not selected for current Windows x64 graph'
        status=review(license,name,coverage) if modes else 'VERIFIED: not in filtered Windows graph; cached declaration recorded, notices not staged for this target.'
        records.append({'ecosystem':'cargo','name':name,'version':version,'locks':sorted(locks),'windows_uses':uses,'distribution':delivery,'declared_license':license,'declared_license_file':data.get('license-file'),'repository':data.get('repository'),'evidence':{'manifest_sha256':manifest_hash,'source':'local Cargo registry package manifest; registry checksum in lockfile','vcs':vcs,'registry_archive':archive_check},'notices':evidence,'obligations':obligations(license),'compatibility_review':status})
    lock=load(ROOT/'package-lock.json');inputs['package-lock.json']=sha((ROOT/'package-lock.json').read_bytes())
    packages=lock['packages'];frontend=set();todo=['react','react-dom','lucide-react','@tauri-apps/api','@tauri-apps/plugin-dialog']
    while todo:
        name=todo.pop();key='node_modules/'+name
        if key in frontend or key not in packages:continue
        frontend.add(key);todo.extend(packages[key].get('dependencies',{}))
    for key,value in sorted(packages.items()):
        if not key:continue
        name=key.split('node_modules/')[-1];version=value['version'];directory=ROOT/key;actual=load(directory/'package.json') if (directory/'package.json').exists() else {}
        evidence=[copy_evidence(p,directory,Path('npm')/f'{name.replace("/","__")}-{version}') for p in license_files(directory)] if actual else []
        archive_check={'available':False}
        integrity=value.get('integrity','')
        if integrity.startswith('sha512-'):
            digest=base64.b64decode(integrity.split('-',1)[1]).hex()
            for cache in [ROOT/'.npm-cache',Path(os.environ.get('LOCALAPPDATA',''))/'npm-cache']:
                cached=cache/'_cacache/content-v2/sha512'/digest[:2]/digest[2:4]/digest[4:]
                if cached.exists():
                    raw=cached.read_bytes();archive_check={'available':True,'matches_lock':hashlib.sha512(raw).hexdigest()==digest,'sha256':sha(raw)}
                    with tarfile.open(cached) as bundle:
                        for e in evidence:
                            try:e['registry_archive_bytes_match']=bundle.extractfile('package/'+e['source_relative_path']).read()==(ROOT/e['file']).read_bytes()
                            except (KeyError,AttributeError):e['registry_archive_bytes_match']=False
                    break
        license=value.get('license');coverage=any(e['full_text_candidate'] for e in evidence)
        delivery='frontend JavaScript runtime candidate' if key in frontend else 'build-time tool with generated CSS delivered' if name=='tailwindcss' else 'build/development only' if actual else 'optional platform package not installed; not selected for this Windows build'
        status=review(license,name,coverage) if actual else 'VERIFIED: absent optional platform package; declaration only from lock, not a shipped component in this target.'
        if actual and actual.get('version')!=version:status='UNRESOLVED / RELEASE BLOCKER: installed version differs from lock.'
        records.append({'ecosystem':'npm','name':name,'version':version,'distribution':delivery,'declared_license':license,'installed_license':actual.get('license'),'repository':actual.get('repository'),'evidence':{'lock_key':key,'integrity':value.get('integrity'),'resolved':value.get('resolved'),'manifest_sha256':sha((directory/'package.json').read_bytes()) if actual else None,'source':'package-lock.json and installed package.json','registry_archive':archive_check},'notices':evidence,'obligations':obligations(license),'compatibility_review':status})
    # Exact shipped backend notices plus source package documentation, without extracting paths to the filesystem.
    special=[]
    for directory in [ROOT/'src-tauri/vendor/7zip',ROOT/'src-tauri/vendor/shell',ROOT/'src-tauri/vendor/windows-rs',ROOT/'src-tauri/tests/fixtures']:
        for p in directory.iterdir():
            if p.is_file() and (any(p.name.lower().startswith(x) for x in ['license','copying','notice']) or p.name=='README.md'):
                special.append(copy_evidence(p,directory,Path('bundled')/directory.name))
    archive=ROOT/'src-tauri/vendor/7zip/7zip-26.05-source.tar.gz'
    with tarfile.open(archive) as t:
        for member in t.getmembers():
            if member.name.endswith(('DOC/License.txt','DOC/copying.txt','DOC/unRarLicense.txt','DOC/readme.txt')):
                content=t.extractfile(member).read();dest=LEGAL/'upstream/bundled/7zip-source'/Path(member.name).name
                dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(content)
                special.append({'file':dest.relative_to(ROOT).as_posix(),'source_relative_path':member.name,'sha256':sha(content),'bytes':len(content),'archive_sha256':sha(archive.read_bytes())})
    binary=[]
    for p in (ROOT/'src-tauri/vendor').rglob('*'):
        if p.is_file() and p.suffix.lower() in ['.exe','.dll','.gz']:
            binary.append({'file':p.relative_to(ROOT).as_posix(),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size,'delivery':'configured bundle resource' if '/windows-rs/' not in p.as_posix() else 'not an explicit bundle resource'})
    assets=[]
    for directory in ['branding','public','src-tauri/icons','src/assets']:
        for p in (ROOT/directory).rglob('*'):
            if p.is_file() and p.suffix.lower() in ['.png','.ico','.icns','.svg','.woff','.woff2','.ttf','.otf']:
                relative=p.relative_to(ROOT).as_posix()
                approved=directory in ['branding','src-tauri/icons'] or relative=='public/packit-favicon.png'
                assets.append({'file':relative,'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size,'distribution':'Vite public payload (including unreferenced files)' if directory=='public' else 'source-only unused scaffold; not imported by current app' if directory=='src/assets' else 'branding source or generated native/icon build input','rights':'VERIFIED USER CONFIRMATION: original approved PACKIT artwork by Krish Agarwal (2026); derived assets inherit that provenance; GPL-3.0-only.' if approved else 'UNRESOLVED / RELEASE BLOCKER if distributed: scaffold asset provenance/permission unestablished'})
    duplicate=ROOT/'packit-icon.png'
    if duplicate.exists():assets.append({'file':'packit-icon.png','sha256':sha(duplicate.read_bytes()),'bytes':duplicate.stat().st_size,'distribution':'source-root duplicate; not a configured runtime asset','rights':'VERIFIED duplicate of owner-supplied artwork' if sha(duplicate.read_bytes())==sha((ROOT/'branding/packit-icon.png').read_bytes()) else 'UNRESOLVED: not byte-identical to approved icon; provenance required if conveyed'})

    result={'target':'x86_64-pc-windows-msvc','app_version':load(ROOT/'src-tauri/tauri.conf.json')['version'],'scope':'configured Windows build; final installer not produced or audited','license_identifier':'GPL-3.0-only','inputs':inputs,'records':records,'bundled_evidence':special,'bundled_artifacts':binary,'assets':assets,'npm_root_version_drift':{'package':load(ROOT/'package.json')['version'],'lock':lock['version'],'lock_root':lock['packages']['']['version']}}
    write(LEGAL/'inventory.json',json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    lines=['# Dependency license inventory','', '> Evidence inventory, not legal clearance. Windows x64 declared graph is conservative; final package inclusion remains to be checked. Non-Windows/unused lock records are retained for complete lock coverage.','',f'{len([r for r in records if r["ecosystem"]=="cargo"])} third-party Cargo name/version records; {len([r for r in records if r["ecosystem"]=="npm"])} npm lock records. See [inventory.json](inventory.json) for checksums, source/repository evidence, per-component obligations, contexts and exact notice files.','', '## Reading the findings','', 'VERIFIED means the stated evidence/check was verified, not that every embedded component is legally cleared. UNRESOLVED findings on distributed candidates block release. Build-only packages are not automatically shipped; generated code/CSS may retain notices. Keep the collected notices conservatively. Legacy slash license expressions are reproduced verbatim, not silently rewritten. No package-level license relabels embedded native code.','', '| Ecosystem / component | Declared license | Windows distribution assessment | Evidence and review |','| --- | --- | --- | --- |']
    for r in records:
        license=(r['declared_license'] or 'UNKNOWN').replace('|','\\|');refs=', '.join(f'[{Path(e["file"]).name}]({Path(e["file"]).relative_to("legal").as_posix()})' for e in r['notices'][:2]) or 'Manifest/lock evidence only'
        flag='UNRESOLVED' if r['compatibility_review'].startswith('UNRESOLVED') else 'VERIFIED evidence'
        lines.append(f'| {r["ecosystem"]}: {r["name"]} {r["version"]} | {license} | {r["distribution"]} | {refs}; {flag} |')
    write(LEGAL/'DEPENDENCY_LICENSES.md','\n'.join(lines)+'\n')
    # Byte-level manifest also covers staged documents/metadata without any local-user absolute paths.
    evidence_files={e['file']:e for r in records for e in r['notices']}
    evidence_files.update({e['file']:e for e in special})
    for e in old_evidence:
        if e['file'].startswith(('legal/upstream/toolchain/','legal/upstream/development/')) and (ROOT/e['file']).is_file():evidence_files[e['file']]=e
    for e in old_evidence:
        if e['file'] not in evidence_files:
            stale=ROOT/e['file']
            if stale.resolve().is_relative_to((LEGAL/'upstream').resolve()) and stale.is_file() and not stale.is_symlink():stale.unlink()
    write(LEGAL/'evidence-manifest.json',json.dumps({'files':[evidence_files[k] for k in sorted(evidence_files)]},indent=2)+'\n')
    print(f'Inventory generated: {len(records)} package records, {len(evidence_files)} preserved notice/evidence files, {len(binary)} native artifacts, {len(assets)} assets.')
    unresolved=[r for r in records if r['compatibility_review'].startswith('UNRESOLVED')];print('Unresolved package-level findings:',len(unresolved));print('\n'.join(f"{r['ecosystem']} {r['name']} {r['version']} ({r['distribution']})" for r in unresolved))
def check():
    errors=[];inv=load(LEGAL/'inventory.json');evidence=load(LEGAL/'evidence-manifest.json')
    for path,expected in inv['inputs'].items():
        if sha((ROOT/path).read_bytes())!=expected:errors.append('Inventory stale: '+path)
    for r in inv['records']:
        a=r['evidence'].get('registry_archive',{})
        if a.get('available') and not a.get('matches_lock'):errors.append('Archive differs from lock: '+r['name'])
    for e in evidence['files']:
        if e.get('registry_archive_bytes_match') is False:errors.append('Cached notice differs from registry archive: '+e['file'])
        p=ROOT/e['file']
        if not p.is_file() or sha(p.read_bytes())!=e['sha256']:errors.append('Notice missing/changed: '+e['file'])
    official=load(LEGAL/'official-texts.json')
    for e in official['files']:
        if sha((ROOT/e['file']).read_bytes())!=e['sha256']:errors.append('Official license changed: '+e['file'])
    for p in ['package.json','package-lock.json']:
        data=load(ROOT/p)
        if data.get('license')!='GPL-3.0-only':errors.append('First-party license metadata mismatch: '+p)
    if load(ROOT/'package-lock.json')['packages'][''].get('license')!='GPL-3.0-only':errors.append('npm root package license missing')
    for path in ['src-tauri/Cargo.toml','windows-shell/Cargo.toml']:
        if tomllib.loads((ROOT/path).read_text())['package'].get('license')!='GPL-3.0-only':errors.append('Cargo GPL identifier mismatch')
    first=load(LEGAL/'first-party-spdx.json')
    for path in first['files']:
        head=(ROOT/path).read_text(encoding='utf-8')[:250]
        if 'SPDX-License-Identifier: GPL-3.0-only' not in head or 'SPDX-FileCopyrightText: 2026 Krish Agarwal' not in head:errors.append('Missing first-party SPDX attribution: '+path)
    for ecosystem,lockpath in [('cargo','src-tauri/Cargo.lock'),('cargo','windows-shell/Cargo.lock')]:
        expected={(p['name'],p['version']) for p in tomllib.loads((ROOT/lockpath).read_text())['package'] if p['name'] not in ['packit','packit-shell']}
        actual={(r['name'],r['version']) for r in inv['records'] if r['ecosystem']==ecosystem}
        if not expected<=actual:errors.append('Cargo lock coverage incomplete')
    if len([r for r in inv['records'] if r['ecosystem']=='npm'])!=len(load(ROOT/'package-lock.json')['packages'])-1:errors.append('npm lock coverage incomplete')
    for e in inv['bundled_artifacts']+inv['assets']:
        if e.get('excluded'):
            exclusions=load(LEGAL/'distribution-exclusions.json')
            if e['file'] not in {x['file'] for x in exclusions['removed_assets']} or (ROOT/e['file']).exists():errors.append('Invalid asset exclusion: '+e['file'])
            continue
        if sha((ROOT/e['file']).read_bytes())!=e['sha256']:errors.append('Bundled artifact/asset changed: '+e['file'])
    for path in ['LICENSE','THIRD_PARTY_NOTICES.md','legal/DEPENDENCY_LICENSES.md','legal/SOURCE_DISTRIBUTION.md','legal/COPYRIGHT.md','legal/RELEASE_REVIEW.md']:
        if not (ROOT/path).is_file() or not (ROOT/path).read_text(encoding='utf-8').strip():errors.append('Missing legal document: '+path)
    print('\n'.join(errors) if errors else f'PASS: lock coverage, metadata identifiers, official text hashes, {len(evidence["files"])} byte-preserved notices and bundled hashes. This is NOT release clearance.')
    if errors:return 1
    print('OPEN RELEASE REVIEW: see legal/RELEASE_REVIEW.md; unRAR/aggregation, static-loader source/exception, final source delivery and payload review are not waived by this check.')
    return 0
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('mode',choices=['generate','check']);args=parser.parse_args()
    if args.mode=='generate':generate()
    else:sys.exit(check())

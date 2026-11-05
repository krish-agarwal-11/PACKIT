# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Preserve only newly introduced updater dependency evidence; do not rerun the broad audit."""
import json,hashlib,tarfile,tomllib
from pathlib import Path
from audit_licenses import ROOT,LEGAL,license_files,copy_evidence,obligations,runtime_graph
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
inv=read(LEGAL/'inventory.json');manifest=read(LEGAL/'evidence-manifest.json');meta=read(ROOT/'experiments/license-audit/cargo-main.json');packages,states=runtime_graph(meta);selected={(p['name'],p['version']):(p,states.get(p['id'],set())) for p in packages.values()};existing={(r['name'],r['version']) for r in inv['records'] if r['ecosystem']=='cargo'};added=read(LEGAL/'updater-dependency-delta.json')['records'] if (LEGAL/'updater-dependency-delta.json').exists() else []
for package in tomllib.loads((ROOT/'src-tauri/Cargo.lock').read_text())['package']:
 name,version=package['name'],package['version']
 if 'checksum' not in package or (name,version) in existing:continue
 source,modes=selected.get((name,version),(None,set()));notices=[];license=None;archive_info={};vcs={}
 if source:
  directory=Path(source['manifest_path']).parent;data=tomllib.loads((directory/'Cargo.toml').read_text())['package'];license=data.get('license');archive=directory.parent.parent.parent/'cache'/directory.parent.name/f'{name}-{version}.crate';raw=archive.read_bytes();assert hashlib.sha256(raw).hexdigest()==package['checksum'];archive_info={'available':True,'matches_lock':True,'sha256':package['checksum'],'lock_checksum':package['checksum']}
  with tarfile.open(archive) as t:
   for path in license_files(directory):
    entry=copy_evidence(path,directory,Path('updater')/f'{name}-{version}');assert t.extractfile(f'{name}-{version}/'+entry['source_relative_path']).read()==path.read_bytes();entry['registry_archive_bytes_match']=True;notices.append(entry);manifest['files'].append(entry)
  if (directory/'.cargo_vcs_info.json').exists():vcs=read(directory/'.cargo_vcs_info.json')
 row={'ecosystem':'cargo','name':name,'version':version,'locks':['src-tauri/Cargo.lock'],'windows_uses':{'main':sorted(modes)} if modes else {},'distribution':'updater Windows dependency' if modes else 'not selected for current Windows graph','declared_license':license,'repository':source.get('repository') if source else None,'evidence':{'source':'exact locked source archive / selected Cargo metadata','vcs':vcs,'registry_archive':archive_info},'notices':notices,'obligations':obligations(license) if source else 'Not conveyed in this Windows target; obtain exact terms if selecting this platform package.','compatibility_review':'VERIFIED evidence only: exact registry notice bytes preserved; not new legal clearance.' if notices else 'PREPARED: unselected platform lock entry only; not distributed on Windows.'}
 if modes and not notices:raise SystemExit('Missing selected updater notices: '+name)
 inv['records'].append(row);added.append(row)
# Existing lock records can become runtime dependencies even without new versions.
for row in inv['records']:
 if row['ecosystem']!='cargo' or (row['name'],row['version']) not in selected:continue
 source,modes=selected[row['name'],row['version']]
 if not modes or row.get('windows_uses'):continue
 name,version=row['name'],row['version'];directory=Path(source['manifest_path']).parent;archive=directory.parent.parent.parent/'cache'/directory.parent.name/f'{name}-{version}.crate';checksum=hashlib.sha256(archive.read_bytes()).hexdigest();assert checksum==next(x['checksum'] for x in tomllib.loads((ROOT/'src-tauri/Cargo.lock').read_text())['package'] if x['name']==name and x['version']==version)
 notices=[]
 with tarfile.open(archive) as t:
  for path in license_files(directory):
   entry=copy_evidence(path,directory,Path('updater')/f'{name}-{version}');assert t.extractfile(f'{name}-{version}/'+entry['source_relative_path']).read()==path.read_bytes();entry['registry_archive_bytes_match']=True;notices.append(entry);manifest['files'].append(entry)
 if not notices:raise SystemExit('Missing newly-selected dependency notices: '+name)
 row['windows_uses']={'main':sorted(modes)};row['distribution']='newly selected Windows updater dependency';row['notices']=notices;row['compatibility_review']='VERIFIED exact-source notices; newly selected by updater, no final legal clearance';added.append(row)
for name in inv['inputs']:inv['inputs'][name]=hashlib.sha256((ROOT/name).read_bytes()).hexdigest()
(LEGAL/'inventory.json').write_text(json.dumps(inv,ensure_ascii=False,indent=2)+'\n');unique={e['file']:e for e in manifest['files']};(LEGAL/'evidence-manifest.json').write_text(json.dumps({'files':list(unique.values())},indent=2)+'\n');(LEGAL/'updater-dependency-delta.json').write_text(json.dumps({'scope':'Updater additions only; no broad audit or final legal clearance','records':added},indent=2)+'\n')
base=(LEGAL/'DEPENDENCY_LICENSES.md').read_text().split('\n## Stage 5 updater dependency additions')[0]
with (LEGAL/'DEPENDENCY_LICENSES.md').open('w',encoding='utf-8') as out:
 out.write(base)
 out.write('\n## Stage 5 updater dependency additions\n\nExact evidence: updater-dependency-delta.json. Existing Stage 3B decisions are unchanged.\n\n| Component | Declaration | Selection |\n| --- | --- | --- |\n')
 for row in added:out.write(f"| {row['name']} {row['version']} | {row['declared_license'] or 'unselected / not yet collected'} | {row['distribution']} |\n")
print('Updater delta:',len(added),'new lock records;',sum(bool(r['windows_uses']) for r in added),'selected packages; notices preserved:',sum(len(r['notices']) for r in added))

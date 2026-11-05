# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Focused Stage 3B compliance validation; never legal clearance."""
import argparse,hashlib,json,subprocess,sys,tarfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
load=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--source',default='experiments/license-audit/source-candidate.tar.gz');args=parser.parse_args()
 subprocess.run([sys.executable,str(ROOT/'scripts/audit_licenses.py'),'check'],cwd=ROOT,check=True)
 subprocess.run([sys.executable,str(ROOT/'scripts/restore_test_fixtures.py')],cwd=ROOT,check=True)
 for c in load(ROOT/'legal/mpl-source-review.json')['components']:
  assert sha(ROOT/c['source'])==c['sha256'] and not c['local_modifications'] and not c['exhibit_b_source_markers']
 for q in load(ROOT/'legal/frontend-source-provenance.json')['requests']:
  assert q['status']=='retrieved' and sha(ROOT/q['local'])==q['sha256']
 for c in load(ROOT/'legal/frontend-source-provenance.json')['components']:assert sha(ROOT/c['archive'])==c['sha256']
 exclusions=load(ROOT/'legal/distribution-exclusions.json')
 for e in exclusions['removed_assets']:assert not (ROOT/e['file']).exists()
 for e in exclusions['local_only_fixtures']:assert sha(ROOT/e['file'])==e['sha256']
 expected={'branding/packit-icon.png':'0863313192d2f46310f808fa53c95fce5a3e3bdc8f9ff2366303785c78aebd84','branding/packit-logo.png':'9be7b360483ea3d9c0bdd0d2517c19df575343ac689952300ffaedf48de6a49b'}
 for name,digest in expected.items():assert sha(ROOT/name)==digest
 for p in list((ROOT/'src').rglob('*'))+[ROOT/'index.html']:
  if p.is_file() and p.suffix in ['.ts','.tsx','.css','.html']:
   text=p.read_text(encoding='utf-8-sig')
   for name in ['favicon.svg','icons.svg','vite.svg','hero.png']:assert name not in text,(p,name)
 native=load(ROOT/'legal/NATIVE_COMPONENTS.json');assert len(native['sdk_provenance']['matches'])==9 and all(e['matches_sdk'] for e in native['sdk_provenance']['matches'])
 provenance=load(ROOT/'legal/7zip-release-provenance.json');assert not provenance['substantive_differences'] and all(e['official_bytes_equal'] for e in provenance['binary_comparison']['files'])
 official=ROOT/'legal/sources/7zip/7z2605-src.tar.xz';assert sha(official)==next(e['sha256'] for e in provenance['assets'] if e['name'].endswith('.xz'))
 candidate=ROOT/args.source
 if candidate.exists():
  with tarfile.open(candidate) as t:
   names=set(t.getnames());manifest=json.load(t.extractfile('SOURCE-MANIFEST.json'));
   if manifest.get('build_record'):
    assert manifest['build_record']['source_tree_sha256']==manifest['source_tree_sha256']
    for artifact in manifest['build_record']['artifacts']:assert sha(ROOT/artifact['file'])==artifact['sha256']
   assert not {e['file'] for e in exclusions['local_only_fixtures']+exclusions['removed_assets']}&names
   for e in manifest['files']:
    content=t.extractfile(e['file']).read();assert hashlib.sha256(content).hexdigest()==e['sha256']
   assert all(not n.startswith(('src-tauri/target/','windows-shell/target/','node_modules/','experiments/','dist/')) for n in names)
  print('PASS: source candidate members, exclusions and per-file hashes:',len(names))
 print('PASS: exact notices, lock coverage, native provenance, MPL archives, runtime source hashes, art and exclusions. NOT legal clearance.')
if __name__=='__main__':main()

# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Recover preferred upstream sources for shipped frontend modules, without build tools."""
import concurrent.futures,json,hashlib,base64,tarfile
from recover_license_evidence import ROOT,OUT,fetch
inv=json.loads((ROOT/'legal/inventory.json').read_text());jobs={};records=[]
for r in inv['records']:
 if r['ecosystem']!='npm' or 'frontend JavaScript' not in r['distribution']:continue
 key=r['name'].replace('/','__')+'-'+r['version'];meta=json.loads((OUT/('npm-'+key+'.json')).read_text());archive=OUT/('archive-'+key+'.json');raw=archive.read_bytes();expected=r['evidence']['integrity'].split('-',1)[1];assert base64.b64encode(hashlib.sha512(raw).digest()).decode()==expected
 r['evidence']['registry_archive']={'available':True,'matches_lock':True,'sha256':hashlib.sha256(raw).hexdigest()}
 repo=meta.get('repository');repo=repo['url'] if isinstance(repo,dict) else repo;repo=repo.removeprefix('git+').removesuffix('.git').removeprefix('https://github.com/');commit=meta.get('gitHead')
 if r['name']=='@tauri-apps/api':repo='tauri-apps/tauri';commit='59b63ab500da9859f12d8888c3298bc29bba0b92'
 if r['name']=='lucide-react':repo='lucide-icons/lucide';commit='4b203ec9b6002fd74fd5dadc8612220e64289305'
 assert commit,(key,meta)
 source='preferred-'+repo.replace('/','__')+'-'+commit;jobs[source]='https://codeload.github.com/'+repo+'/tar.gz/'+commit
 records.append({'name':r['name'],'version':r['version'],'archive':archive.relative_to(ROOT).as_posix(),'sha256':hashlib.sha256(raw).hexdigest(),'source_id':source,'repository':repo,'commit':commit,'association':'registry gitHead' if meta.get('gitHead') else 'npm publisher provenance claim; signature not independently verified'})
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:requests=list(pool.map(fetch,jobs.items()))
for q in requests:
 if q['status']=='retrieved':
  with tarfile.open(ROOT/q['local']) as t:assert any(m.name.endswith(('.ts','.tsx','.js')) for m in t.getmembers())
(ROOT/'legal/frontend-source-provenance.json').write_text(json.dumps({'components':records,'requests':requests},indent=2)+'\n');(ROOT/'legal/inventory.json').write_text(json.dumps(inv,ensure_ascii=False,indent=2)+'\n')
print([(q['id'],q['status'],q.get('bytes',q.get('error'))) for q in requests])

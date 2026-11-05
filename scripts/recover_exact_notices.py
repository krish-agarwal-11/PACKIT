# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Second-stage exact Git blob / registry source evidence recovery."""
from recover_license_evidence import OUT, ROOT, fetch
import concurrent.futures,base64,hashlib,json,zipfile
jobs={};blob_info={}
for p in OUT.glob('github-*.json'):
 tree=json.loads(p.read_text())
 for item in tree.get('tree',[]):
  # Repository-wide licenses apply subject to narrower file notices.
  if '/' not in item['path'] and any(item['path'].lower().startswith(t) for t in ['license','copying','notice','copyright']):
   key=p.stem+'__'+item['path'];jobs[key]=item['url'];blob_info[key]={'tree_file':p.relative_to(ROOT).as_posix(),'path':item['path'],'git_blob':item['sha']}
for p in OUT.glob('npm-*.json'):
 v=json.loads(p.read_text());repo=v.get('repository');repo=repo.get('url','') if isinstance(repo,dict) else repo or '';head=v.get('gitHead')
 if head and 'github.com/' in repo:
  repo=repo.split('github.com/',1)[1].removesuffix('.git');jobs[f'npm-tree-{v["name"].replace("/","__")}-{v["version"]}']=f'https://api.github.com/repos/{repo}/git/trees/{head}?recursive=1'
 # Download registry packages needed for existing missing notices and runtime source.
 jobs[f'archive-{v["name"].replace("/","__")}-{v["version"]}']=v['dist']['tarball']
release=json.loads((OUT/'7zip-release-26.05.json').read_text())
for a in release['assets']:
 if a['name'] in ['7z2605-x64.exe','7z2605-src.tar.xz']:jobs[a['name']]=a['browser_download_url']
# fetch writes .json for all but SDK; extension is transport-neutral for archived evidence.
with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:results=list(pool.map(fetch,jobs.items()))
for r in results:
 if r['id'] in blob_info:r['blob_context']=blob_info[r['id']]
 if r['status']=='retrieved' and r['id'] in blob_info:
  encoded=json.loads((ROOT/r['local']).read_text());data=base64.b64decode(encoded['content']);expected=blob_info[r['id']]['git_blob'];assert hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()==expected
  target=ROOT/'legal/upstream/recovered'/r['id'];target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data);r.update(notice=target.relative_to(ROOT).as_posix(),notice_sha256=hashlib.sha256(data).hexdigest())
(ROOT/'legal/recovery-downloads.json').write_text(json.dumps({'requests':results},indent=2)+'\n')
for r in results:print(r['id'],r['status'],r.get('error',r.get('bytes')))

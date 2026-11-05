# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Fetch only version-pinned licensing evidence; failed requests remain evidence gaps."""
import concurrent.futures, hashlib, json, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'experiments/license-audit/stage3b';OUT.mkdir(parents=True,exist_ok=True)
inv=json.loads((ROOT/'legal/inventory.json').read_text())
jobs={}
for r in inv['records']:
 if r['name'] in ['alloc-stdlib','selectors','webview2-com','webview2-com-macros','webview2-com-sys','tauri-plugin'] and r['ecosystem']=='cargo':
  repo=r['repository'].removesuffix('.git').removeprefix('https://github.com/');commit=r['evidence']['vcs']['git']['sha1'];jobs[f'github-{repo.replace("/","__")}-{commit}']=f'https://api.github.com/repos/{repo}/git/trees/{commit}?recursive=1'
 if r['ecosystem']=='npm' and (r['name']=='@tauri-apps/plugin-dialog' or r['compatibility_review'].startswith('UNRESOLVED') or 'frontend JavaScript' in r['distribution']):
  jobs[f'npm-{r["name"].replace("/","__")}-{r["version"]}']=f'https://registry.npmjs.org/{r["name"].replace("/","%2f")}/{r["version"]}'
jobs['7zip-release-26.05']='https://api.github.com/repos/ip7z/7zip/releases/tags/26.05'
jobs['webview2-sdk-1.0.3800.47']='https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/1.0.3800.47/microsoft.web.webview2.1.0.3800.47.nupkg'
def fetch(item):
 key,url=item;result={'id':key,'url':url}
 try:
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'PACKIT-compliance-audit','Accept':'application/json' if key!='webview2-sdk-1.0.3800.47' else '*/*'}),timeout=20) as response:
   data=response.read((64<<20)+1)
   if len(data)>64<<20:raise ValueError('evidence exceeds download limit')
   suffix='.nupkg' if key.startswith('webview2-sdk') else '.json';path=OUT/(key+suffix);path.write_bytes(data)
   result.update(status='retrieved',sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),local=path.relative_to(ROOT).as_posix())
 except Exception as error:result.update(status='unavailable',error=str(error))
 return result
if __name__=='__main__':
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:results=list(pool.map(fetch,jobs.items()))
    (ROOT/'legal/recovery-evidence.json').write_text(json.dumps({'requests':results},indent=2)+'\n')
    for r in results:print(r['id'],r['status'],r.get('error',r.get('bytes')))

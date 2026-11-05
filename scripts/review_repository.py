# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Review proposed Git paths without reading key/credential files or printing secret contents."""
import hashlib,json,re,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
paths=[p for p in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0') if p];issues=[];total=0
for name in sorted(set(paths)):
 path=ROOT/name
 if path.is_symlink():issues.append({'file':name,'reason':'source link requires review'});continue
 if path.suffix.lower() in ['.key','.pem','.pfx','.p12'] or path.name.startswith('.env') or path.name.lower() in ['credentials','credentials.toml','id_rsa','id_ed25519']:
  issues.append({'file':name,'reason':'credential-like filename; contents NOT read'});continue
 total+=path.stat().st_size
 if path.stat().st_size>8*1024*1024:issues.append({'file':name,'reason':'unexpected large file'});continue
 if path.suffix.lower() in ['.png','.ico','.icns','.exe','.dll','.gz','.xz','.crate','.zip','.7z','.rar','.iso']:continue
 try:text=path.read_text(encoding='utf-8')
 except UnicodeError:continue
 if ('-'*5+'BEGIN ') in text and ('PRIVATE KEY'+'-'*5) in text:issues.append({'file':name,'reason':'possible private key body; contents withheld'})
 if re.search(r'gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|AKIA[0-9A-Z]{16}',text):issues.append({'file':name,'reason':'possible credential pattern; contents withheld'})
 if re.search(r'[A-Za-z]:[\\/]+(?:Users|Projects)[\\/]',text,re.I):issues.append({'file':name,'reason':'personal absolute path'})
report={'files':len(set(paths)),'bytes':total,'issues':issues,'scope':'Text pattern/name scan; not a guarantee that arbitrary binary/vendor content contains no secrets. Private key/password files never read.','excluded':'build outputs/caches, experiments, credentials, signing keys, generated shell DLL/manifest and 13 uncertain upstream RAR fixtures','public_key':'src-tauri/updater-public-key.pub is intentionally public and included'}
out=ROOT/'experiments/license-audit/stage5';out.mkdir(parents=True,exist_ok=True);(out/'repository-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(1 if issues else 0)

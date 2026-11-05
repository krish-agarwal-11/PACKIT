# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Prepare a Tauri artifact-signature manifest locally; never signs, uploads or publishes."""
import argparse,base64,hashlib,json,re
from pathlib import Path
REPO='https://github.com/krish-agarwal-11/PACKIT'
def prepare(version,installer,signature):
 if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?',version):raise ValueError('Use an explicit SemVer release version')
 if installer.name!=f'PACKIT_{version}_x64-setup.exe' or installer.read_bytes()[:2]!=b'MZ':raise ValueError('Installer name/version/PE header differs')
 if signature.suffix!='.sig':raise ValueError('Only a public .sig file may be read; never supply a private key')
 text=signature.read_text().strip();decoded=base64.b64decode(text,validate=True).decode('utf-8')
 lines=decoded.splitlines()
 if len(lines)<4 or not lines[0].startswith('untrusted comment:') or not lines[2].startswith('trusted comment:'):raise ValueError('Malformed public signature file')
 if not any(part==f'version:{version}' for part in lines[2].removeprefix('trusted comment:').strip().split('\t')):raise ValueError('Signature must bind this version; owner must sign with --app-version')
 # Structural preparation only; cryptographic verification is performed by the official updater.
 return {'version':version,'notes':'See the GitHub release notes.','platforms':{'windows-x86_64':{'url':f'{REPO}/releases/download/v{version}/{installer.name}','signature':text}}}
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--version',required=True);p.add_argument('--installer',type=Path,required=True);p.add_argument('--signature',type=Path,required=True);p.add_argument('--output',type=Path,default=Path('release-staging/latest.json'));a=p.parse_args();result=prepare(a.version,a.installer,a.signature);a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(result,indent=2)+'\n');print('Prepared locally:',a.output);print('Installer SHA-256:',hashlib.sha256(a.installer.read_bytes()).hexdigest());print('Not published; structural checks do not replace official signature verification.')

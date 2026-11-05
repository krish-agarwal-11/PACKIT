# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Pair the private installer with the captured source tree; no publication or legal clearance."""
import hashlib,json,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
config=read(ROOT/'src-tauri/tauri.conf.json');version=config['version'];stage=ROOT/'experiments/license-audit/stage4';manifest=read(ROOT/'experiments/license-audit/source-candidate-manifest.json');shell=read(ROOT/'src-tauri/vendor/shell/manifest.json')
paths=[f'src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/PACKIT_{version}_x64-setup.exe','src-tauri/target/x86_64-pc-windows-msvc/release/packit.exe','experiments/license-audit/stage4/packit-packaged.exe','src-tauri/vendor/shell/'+shell['filename']]
record={'version':version,'identifier':config['identifier'],'publisher':config['bundle']['publisher'],'scope':'currentUser','target':'x86_64-pc-windows-msvc','profile':'release','features':['tauri/custom-protocol'],'build_command':'npm run installer:build','toolchain':subprocess.check_output(['rustc','-vV'],text=True).strip(),'rustflags':(ROOT/'.cargo/config.toml').read_text(),'nsis':'3.13','source_tree_sha256':manifest['source_tree_sha256'],'artifacts':[{'file':name,'sha256':hashlib.sha256((ROOT/name).read_bytes()).hexdigest(),'bytes':(ROOT/name).stat().st_size} for name in paths],'host':str(sys.getwindowsversion()),'packaged_app_note':'Tauri patches UNK to NSS in the packaged copy and restores the original build executable. All other bytes were compared.','manual_windows11_acceptance':'pending; no installation or real shell registration was performed','public_legal_clearance':False}
output=stage/'build-record.json';output.write_text(json.dumps(record,indent=2)+'\n');print(output);print('Installer SHA-256:',record['artifacts'][0]['sha256'])

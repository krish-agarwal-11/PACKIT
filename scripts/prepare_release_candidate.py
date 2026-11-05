# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Stage local RC artifacts and matching source. Never signs, installs or publishes."""
import hashlib,json,shutil,subprocess,sys,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
load=lambda p:json.loads(p.read_text(encoding="utf-8-sig"))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def run(*args):subprocess.run(args,cwd=ROOT,check=True)
def main():
 config=load(ROOT/"src-tauri/tauri.conf.json");version=config["version"]
 assert version=="1.0.0" and config["identifier"]=="org.packit.desktop"
 out=ROOT/"release-staging"/version;out.mkdir(parents=True,exist_ok=True)
 installer=out/f"PACKIT_{version}_x64-setup.exe"
 original=ROOT/f"src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/{installer.name}"
 # Never replace an already owner-signed artifact or leave a stale signed manifest.
 assert not installer.with_suffix(".exe.sig").exists() and not (out/"latest.json").exists(),"Signed staging exists: preserve it and select a new reviewed candidate"
 shutil.copyfile(original,installer);assert sha(installer)==sha(original)
 source=out/f"PACKIT_{version}_windows-x64-source.tar.gz"
 run(sys.executable,"scripts/prepare_source_bundle.py","--output",source.relative_to(ROOT).as_posix())
 tree=load(ROOT/"experiments/license-audit/source-candidate-manifest.json")["source_tree_sha256"]
 shell=load(ROOT/"src-tauri/vendor/shell/manifest.json")
 artifacts=[installer,ROOT/"src-tauri/target/x86_64-pc-windows-msvc/release/packit.exe",ROOT/"experiments/license-audit/stage4/packit-packaged.exe",ROOT/"src-tauri/vendor/shell"/shell["filename"]]
 assert artifacts[2].is_file(),"Run the packaged payload comparison first"
 record={"version":version,"identifier":config["identifier"],"publisher":config["bundle"]["publisher"],"scope":"currentUser","target":"x86_64-pc-windows-msvc","profile":"release","features":["tauri/custom-protocol"],"build_command":"npm run installer:build","toolchain":subprocess.check_output(["rustc","-vV"],text=True).strip(),"source_tree_sha256":tree,"base_commit":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),"branch":subprocess.check_output(["git","branch","--show-current"],cwd=ROOT,text=True).strip(),"workspace_changes":subprocess.check_output(["git","status","--porcelain"],cwd=ROOT,text=True).strip() or "clean tracked tree; source manifest also records generated build inputs","artifacts":[{"file":p.relative_to(ROOT).as_posix(),"sha256":sha(p),"bytes":p.stat().st_size} for p in artifacts],"packaged_app_note":"Tauri changes the UNK bundle marker to NSS in its packaged copy; the payload audit checks all other bytes.","manual_acceptance":"Pending for this exact 1.0.0 candidate","public_legal_clearance":False,"updater_feed_ready":load(ROOT/"src-tauri/update-policy.json")["feedReady"],"authenticode_signed":False}
 record_path=out/"build-record.json";record_path.write_text(json.dumps(record,indent=2)+"\n")
 run(sys.executable,"scripts/prepare_source_bundle.py","--output",source.relative_to(ROOT).as_posix(),"--build-record",record_path.relative_to(ROOT).as_posix())
 legal=out/f"PACKIT_{version}_legal-materials.zip"
 with zipfile.ZipFile(legal,"w",compression=zipfile.ZIP_DEFLATED) as z:
  paths=[ROOT/"LICENSE",ROOT/"THIRD_PARTY_NOTICES.md"]+sorted(p for p in (ROOT/"legal").rglob("*") if p.is_file())
  for p in paths:
   assert not p.is_symlink()
   info=zipfile.ZipInfo(p.relative_to(ROOT).as_posix(),date_time=(1980,1,1,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o644<<16;z.writestr(info,p.read_bytes())
 assets=[installer,source,legal,record_path]
 inventory={"version":version,"status":"Local RC only; NOT published, updater signed or legally cleared","assets":[{"file":p.name,"sha256":sha(p),"bytes":p.stat().st_size,"purpose":"local build evidence" if p==record_path else "candidate release asset"} for p in assets],"pending":["owner signature .exe.sig","official signature verification","latest.json","manual acceptance","clean source reconstruction","two documented distribution interpretation decisions","genuine README screenshots"]}
 inventory_path=out/"asset-inventory.json";inventory_path.write_text(json.dumps(inventory,indent=2)+"\n")
 assets.append(inventory_path)
 (out/"SHA256SUMS.txt").write_text("".join(sha(p)+"  "+p.name+"\n" for p in assets))
 print(json.dumps(inventory,indent=2))
if __name__=="__main__":main()

# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Owner-side public-signature manifest/inventory finalization. Never reads signing keys."""
import hashlib,json
from pathlib import Path
from prepare_release_manifest import prepare
ROOT=Path(__file__).resolve().parent.parent
def main():
 out=ROOT/"release-staging/1.0.0"
 inventory_path=out/"asset-inventory.json";inventory=json.loads(inventory_path.read_text())
 assert inventory["version"]=="1.0.0"
 for row in inventory["assets"]:
  name=row["file"];assert Path(name).name==name
  assert hashlib.sha256((out/name).read_bytes()).hexdigest()==row["sha256"],"Prepared asset changed: "+name
 installer=out/"PACKIT_1.0.0_x64-setup.exe";signature=out/(installer.name+".sig")
 expected=prepare("1.0.0",installer,signature)
 assert json.loads((out/"latest.json").read_text())==expected,"Manifest does not match exact public signature and release artifact"
 for p in [signature,out/"latest.json"]:
  row={"file":p.name,"sha256":hashlib.sha256(p.read_bytes()).hexdigest(),"bytes":p.stat().st_size,"purpose":"owner-prepared updater release asset"}
  inventory["assets"]=[r for r in inventory["assets"] if r["file"]!=p.name]+[row]
 inventory["pending"]=[p for p in inventory["pending"] if p not in ["owner signature .exe.sig","latest.json"]]
 inventory["signature_note"]="Genuine public signature present; owner must successfully run official updater signature verification before publication. This script checks structure/pairing, not cryptography."
 inventory_path.write_text(json.dumps(inventory,indent=2)+"\n")
 assets=[out/r["file"] for r in inventory["assets"]]+[inventory_path]
 (out/"SHA256SUMS.txt").write_text("".join(hashlib.sha256(p.read_bytes()).hexdigest()+"  "+p.name+"\n" for p in assets))
 print("Public signature/manifest paired; hashes refreshed. No signing, key access, upload or publication.")
if __name__=="__main__":main()

"""Small real ISO fixtures; development-only pycdlib 1.16.0 (LGPL-2.1)."""
import io
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/"experiments/iso-fixture-tools"))
import pycdlib
root=Path(__file__).resolve().parent
for variant in ["iso9660","joliet","rockridge"]:
    iso=pycdlib.PyCdlib()
    iso.new(interchange_level=1, joliet=3 if variant=="joliet" else None, rock_ridge="1.09" if variant=="rockridge" else None, vol_ident="PACKIT_TEST")
    folder=dict(iso_path="/NESTED")
    file=dict(iso_path="/NESTED/FILE.TXT;1")
    empty=dict(iso_path="/EMPTY")
    if variant=="joliet":
        folder["joliet_path"]="/nested";file["joliet_path"]="/nested/caf\u00e9.txt";empty["joliet_path"]="/empty"
    if variant=="rockridge":
        folder["rr_name"]="nested";file["rr_name"]="caf\u00e9.txt";empty["rr_name"]="empty"
    iso.add_directory(**folder);iso.add_directory(**empty)
    data=b"PACKIT ISO real payload\n"
    iso.add_fp(io.BytesIO(data),len(data),**file)
    iso.write(str(root/(variant+".iso")));iso.close()

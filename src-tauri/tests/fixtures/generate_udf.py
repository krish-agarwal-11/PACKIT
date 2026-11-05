"""Reproduce a real physical-partition UDF image (no mock archive data)."""
import io
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "experiments/iso-fixture-tools"))
import pycdlib
root = Path(__file__).resolve().parent
archive = pycdlib.PyCdlib()
# pycdlib's 2.60 writer emits a physical layout with minimum read revision 1.02.
# PACKIT reports that actual on-disc minimum read revision, not the writer label.
archive.new(udf="2.60", vol_ident="PACKIT_UDF")
archive.add_directory(iso_path="/NESTED", udf_path="/nested")
archive.add_directory(iso_path="/EMPTY", udf_path="/empty")
data = b"PACKIT original UDF payload\n"
archive.add_fp(io.BytesIO(data), len(data), iso_path="/NESTED/CAFE.TXT;1", udf_path="/nested/caf\u00e9-\u65e5\u672c.txt")
archive.add_fp(io.BytesIO(b""), 0, iso_path="/ZERO.TXT;1", udf_path="/zero.txt")
archive.write(str(root / "udf260.iso"))
archive.close()

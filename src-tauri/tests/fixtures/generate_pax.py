"""Regenerate original, small Python-default POSIX PAX fixtures."""
import gzip
import io
from pathlib import Path
import tarfile

root = Path(__file__).resolve().parent
payload = b"real Python PAX payload\n"
name = "nested/" + "long-folder-" * 10 + "/caf\u00e9-\u65e5\u672c.txt"
stream = io.BytesIO()
# Deliberately use tarfile's default PAX format, including global/local records.
with tarfile.open(fileobj=stream, mode="w", pax_headers={"comment": "PACKIT deterministic PAX fixture", "uid": "123"}) as archive:
    for folder in ["nested", name.rsplit("/", 1)[0]]:
        entry = tarfile.TarInfo(folder)
        entry.type = tarfile.DIRTYPE
        entry.mtime = 1700000000.125
        archive.addfile(entry)
    entry = tarfile.TarInfo(name)
    entry.size = len(payload)
    entry.mtime = 1700000000.125
    archive.addfile(entry, io.BytesIO(payload))
raw = stream.getvalue()
(root / "python-pax.tar").write_bytes(raw)
with (root / "python-pax.tar.gz").open("wb") as target:
    with gzip.GzipFile(filename="", mode="wb", fileobj=target, mtime=0) as compressed:
        compressed.write(raw)

# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
"""Verify local-only test fixtures. --fetch explicitly restores missing upstream data, not redistribution permission."""
import argparse,binascii,hashlib,json,struct,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent

def decode(data):
 lines=data.splitlines();start=next(i for i,l in enumerate(lines) if l.startswith(b'begin '));out=bytearray()
 for line in lines[start+1:]:
  if line==b'end':break
  out.extend(binascii.a2b_uu(line))
 return bytes(out)
def rar4_member(data):
 result=data[:7];offset=7
 while offset+7<=len(data):
  kind=data[offset+2];flags,size=struct.unpack_from('<HH',data,offset+3);extra=struct.unpack_from('<I',data,offset+7)[0] if flags&0x8000 else 0
  assert size>=7 and offset+size+extra<=len(data)
  keep=kind in [0x73,0x7b]
  if kind==0x74:
   length=struct.unpack_from('<H',data,offset+26)[0];name=offset+32+(8 if flags&0x100 else 0);keep=data[name:name+length].replace(b'\\',b'/')==b'testdir/test.txt'
  if keep:result+=data[offset:offset+size+extra]
  offset+=size+extra
 return result
if __name__=='__main__':
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--fetch',action='store_true');args=parser.parse_args()
 for entry in json.loads((ROOT/'legal/distribution-exclusions.json').read_text())['local_only_fixtures']:
  path=ROOT/entry['file']
  if not path.exists():
   if not args.fetch:raise SystemExit('Missing local test data: '+entry['file']+'; optional --fetch requires network and upstream terms review')
   with urllib.request.urlopen(entry['upstream_url'],timeout=20) as response:raw=response.read(4*1024*1024+1)
   assert len(raw)<=4*1024*1024
   raw=decode(raw)
   if entry['transform']=='rar4-single-member':raw=rar4_member(raw)
   assert hashlib.sha256(raw).hexdigest()==entry['sha256'],'Upstream fixture bytes differ'
   path.parent.mkdir(parents=True,exist_ok=True)
   with path.open('xb') as output:output.write(raw)
  assert not path.is_symlink() and hashlib.sha256(path.read_bytes()).hexdigest()==entry['sha256']
 print('PASS: 13 local fixture hashes; no permissions inferred; no files changed without --fetch')

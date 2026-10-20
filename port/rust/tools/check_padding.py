#!/usr/bin/env python3
"""Compare the two TL string padding rules on a real Bot API file_id.

TL serialisation pads a field to a 4-byte boundary, so for a `string` the pad is
computed from `len + <length-prefix bytes>`. For a 1-byte length that is
`(4 - ((len + 1) % 4)) % 4`, NOT `(4 - (len % 4)) % 4`.
"""
import base64
import struct
import sys

fid = sys.argv[1] if len(sys.argv) > 1 else (
    "BQACAgUAAxkDASkV1mrVoxtmjeajGGVQGazxMmuiPS5kAAJzBQACk8FIV9cDMiae-wJuPQQ"
)

raw = base64.urlsafe_b64decode(fid + "=" * (-len(fid) % 4))
out = []
last = -1
last_was_null = False
for cur in raw:
    if last_was_null:
        out += [0] * cur
        last_was_null = False
        last = -1
    else:
        if last >= 0:
            out.append(last)
        last = cur
        last_was_null = cur == 0
if last >= 0:
    out.append(last)

print("decoded_len", len(out), "version", out[-1], "sub", out[-2])
type_raw = struct.unpack("<I", bytes(out[0:4]))[0]
dc = struct.unpack("<i", bytes(out[4:8]))[0]
print(f"type_raw=0x{type_raw:x} has_ref={bool(type_raw & (1 << 25))} dc={dc}")

pos = 8
ln = out[pos]
pos += 1
if ln == 254:
    ln = out[pos] | (out[pos + 1] << 8) | (out[pos + 2] << 16)
    pos += 3
ref = bytes(out[pos:pos + ln])
print("reflen", ln, "ref", ref.hex())
body_start = pos
prefix = pos - 8  # bytes consumed by the length field itself

pad_tl = (4 - ((prefix + ln) % 4)) % 4
pad_wrong = (4 - (ln % 4)) % 4
for name, pad in (("tl-correct", pad_tl), ("port-current", pad_wrong)):
    p = body_start + ln + pad
    idb = bytes(out[p:p + 8])
    ah = bytes(out[p + 8:p + 16])
    print(f"{name:13s} pad={pad} id={struct.unpack('<q', idb)[0]} access={struct.unpack('<q', ah)[0]}")

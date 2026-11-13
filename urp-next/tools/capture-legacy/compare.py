#!/usr/bin/env python3
"""Measure byte and RGBA channel stability between two independent captures."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib


def rgba(path):
    data = path.read_bytes()
    pos, chunks = 8, []
    while pos < len(data):
        n, tag = struct.unpack_from('>I4s', data, pos)
        body = data[pos + 8:pos + 8 + n]
        pos += n + 12
        if tag == b'IHDR':
            w, h, depth, kind, _, _, interlace = struct.unpack('>IIBBBBB', body)
        elif tag == b'IDAT':
            chunks.append(body)
    if depth != 8 or kind not in (2, 6) or interlace:
        raise ValueError('expected non-interlaced RGB/RGBA PNG: ' + str(path))
    channels = 4 if kind == 6 else 3
    stride = w * channels
    raw = zlib.decompress(b''.join(chunks))
    rows, previous = [], bytearray(stride)
    for y in range(h):
        offset = y * (stride + 1)
        mode = raw[offset]
        row = bytearray(raw[offset + 1:offset + 1 + stride])
        for x in range(stride):
            left = row[x - channels] if x >= channels else 0
            up = previous[x]
            diagonal = previous[x - channels] if x >= channels else 0
            if mode == 0:
                break
            if mode == 1:
                predictor = left
            elif mode == 2:
                predictor = up
            elif mode == 3:
                predictor = (left + up) // 2
            elif mode == 4:
                p = left + up - diagonal
                distances = [abs(p - left), abs(p - up), abs(p - diagonal)]
                predictor = [left, up, diagonal][distances.index(min(distances))]
            else:
                raise ValueError('bad PNG filter')
            row[x] = (row[x] + predictor) & 255
        previous = row
        if channels == 3:
            rows.append(b''.join(row[x:x + 3] + b'\xff' for x in range(0, stride, 3)))
        else:
            rows.append(bytes(row))
    return w, h, b''.join(rows)


def compare(first, second):
    files_a = {str(p.relative_to(first)) for p in first.rglob('*') if p.is_file()}
    files_b = {str(p.relative_to(second)) for p in second.rglob('*') if p.is_file()}
    if files_a != files_b:
        raise ValueError('different file sets: ' + repr(sorted(files_a ^ files_b)))
    rows = []
    for name in sorted(files_a):
        a, b = first / name, second / name
        exact = a.read_bytes() == b.read_bytes()
        row = {'path': name, 'exact': exact, 'sha256': hashlib.sha256(a.read_bytes()).hexdigest(),
               'second_sha256': hashlib.sha256(b.read_bytes()).hexdigest()}
        if name.endswith('.png'):
            wa, ha, aa = rgba(a)
            wb, hb, bb = rgba(b)
            if (wa, ha) != (wb, hb):
                raise ValueError('frame dimensions changed: ' + name)
            row['max_channel_difference_rgba'] = [max((abs(x-y) for x,y in zip(aa[c::4], bb[c::4])), default=0) for c in range(4)]
        rows.append(row)
    return {'all_byte_identical': all(r['exact'] for r in rows), 'files': rows}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('first', type=Path)
    ap.add_argument('second', type=Path)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    result = compare(a.first, a.second)
    a.out.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'all_byte_identical': result['all_byte_identical'],
                      'differing': [r for r in result['files'] if not r['exact']]}, indent=2))


if __name__ == '__main__':
    main()

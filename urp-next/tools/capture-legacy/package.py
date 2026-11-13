#!/usr/bin/env python3
"""Package an observed pair of captures, retaining determinism measurements."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
from compare import compare


def dump(path, value):
    path.write_text(json.dumps(value, sort_keys=True, ensure_ascii=False, indent=2) + '\n')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('first', type=Path)
    ap.add_argument('second', type=Path)
    ap.add_argument('out', type=Path)
    a = ap.parse_args()
    measurement = compare(a.first, a.second)
    shutil.copytree(a.first, a.out, dirs_exist_ok=False)
    dump(a.out / 'determinism.json', measurement)
    # Preserve complete per-glyph records without spending the repository budget on repeated JSON keys.
    text = a.out / 'text/manifest.json'
    if text.exists():
        (text.parent / 'manifest.json.gz').write_bytes(gzip.compress(text.read_bytes(), mtime=0))
        text.unlink()
    provenance = json.loads((a.out / 'provenance.json').read_text())
    sets = {}
    command = ('urp-next/tools/capture-legacy/environment.sh urp-next/tools/capture-legacy/regenerate.sh '
               '"$OLD_REPO" "$OUTPUT" "$SCRATCH"')
    descriptions = {
        'frames': 'Headless in-engine frame PNGs at the existing harness route checkpoints. Movies and animated GIF checkpoints are excluded. The deliberate screen-loop plan is held with auto off before its nonterminating advance command.',
        'shaders': 'All 25 built-in part combinations in desktop GLSL120 and ESSL100: 50 programs, 100 translated stages, original stage strings, source parts and MIT attribution. These are the existing extractor battery, not every possible runtime combination.',
        'text': 'Native FT/HB glyph metrics and isolated glyph raster hashes, engine layout coordinates and break flags for synthetic text cases. manifest.json.gz is ordinary UTF-8 JSON compressed with a fixed gzip timestamp.',
        'surface': 'Raw RGBA8 outputs of fixed small pygame Surface operations, plus pixel format and dirty-rectangle metadata.',
        'traces': 'Ordered harness say/text-hash/label/choice/advance progress, with command acknowledgements and polling-only state excluded.'}
    for name, description in descriptions.items():
        directory = a.out / name
        if not directory.exists():
            continue
        rows = [row for row in measurement['files'] if row['path'].startswith(name + '/')]
        exact = all(row['exact'] for row in rows)
        bound = [max((row.get('max_channel_difference_rgba', [0]*4)[c] for row in rows), default=0) for c in range(4)]
        sets[name] = {'comparison': 'exact' if exact else 'tolerance', 'max_channel_difference_rgba': bound,
                      'regenerate': command, 'observed_files': len(rows)}
        (directory / 'README.md').write_text('# Legacy ' + name + '\n\n' + description + '\n\n'
            + 'Regenerate from the old repository root (fresh output and scratch directories):\n\n```sh\n' + command + '\n```\n\n'
            + 'Source: `' + provenance['source_commit'] + '`. Host: `' + provenance['host'] + '`, '
            + provenance['architecture'] + '. Renderer: ' + provenance['renderer'] + '.\n'
            + 'Two separate process captures: ' + ('byte-identical.' if exact else 'not byte-identical; see ../determinism.json for every differing checkpoint and measured RGBA bounds.') + '\n')
    files = []
    for path in sorted(a.out.rglob('*')):
        if path.is_file():
            name = path.relative_to(a.out).as_posix()
            files.append({'path': name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'size': path.stat().st_size})
    for name, info in sets.items():
        subset = [f for f in files if f['path'].startswith(name + '/')]
        info['files'] = len(subset)
        info['size'] = sum(f['size'] for f in subset)
    dump(a.out / 'MANIFEST.json', {'schema_version': 1, 'source_commit': provenance['source_commit'],
         'host': provenance['host'], 'renderer': provenance['renderer'], 'sets': sets, 'files': files,
         'manifest_note': 'This manifest lists all other files; it cannot hash itself.',
         'omissions': ['Media movie frames and animated GIF frames are not still checkpoints.',
                       'Shader battery uses all built-in source parts through 25 existing harness combinations, not the combinatorial runtime shader space.']})
    print(json.dumps(sets, indent=2))


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Capture the old player's harness checkpoints without a window or compositor."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
PLANS = {'story': 'harness/plans/route.plan', 'text': 'harness/testgames/text/text.plan',
         'view': 'harness/testgames/view/view.plan', 'media': 'harness/testgames/media/media.plan',
         'tl': 'harness/testgames/tl/tl.plan', 'py7': 'harness/testgames/py7/py7.plan',
         'loop': 'harness/testgames/loop/screen.plan'}


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def dump(p, value):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + '\n')


class Run:
    def __init__(self, repo, binary, game, scratch):
        self.hz = scratch / 'harness'
        self.hz.mkdir(parents=True)
        self.log = open(scratch / 'player.log', 'w')
        env = dict(os.environ, PLAYER_HEADLESS='1', HARNESS_DIR=str(self.hz),
                   SDL_AUDIODRIVER='dummy', RENPY_DISABLE_BACKUPS='I take responsibility for this.')
        self.proc = subprocess.Popen([str(binary), str(game), '--data', str(scratch / 'data'),
            '--logdir', str(scratch / 'logs'), '--harness-script', str(repo / 'harness/rpy/zz_harness.rpy'),
            '--renderer', 'wgpu'], env=env, stdout=self.log, stderr=subprocess.STDOUT)
        self.mark = 0

    def lines(self):
        p = self.hz / 'progress.txt'
        return p.read_text().splitlines() if p.exists() else []

    def wait(self, token, mark=None, seconds=120):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            lines = self.lines()[self.mark if mark is None else mark:]
            errors = [s for s in lines if s.startswith(('cmd-error ', 'poll-error '))]
            if errors:
                raise RuntimeError('\n'.join(errors))
            if any(s == token or s.startswith(token + ' ') for s in lines):
                return
            if self.proc.poll() is not None:
                raise RuntimeError('player exited: ' + str(self.proc.returncode))
            time.sleep(.05)
        raise RuntimeError('timeout waiting for %s; last lines %r' % (token, self.lines()[-12:]))

    def cmd(self, command, done=False):
        self.mark = len(self.lines())
        p = self.hz / 'pending.txt'
        p.write_text(command + '\n')
        p.replace(self.hz / 'cmd.txt')
        self.wait('cmd-ack ' + command)
        if done:
            self.wait('cmd-done ' + command)

    def shot(self, path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self.cmd('exec renpy.pygame.image.save(renpy.display.draw.screenshot(None), %r)' % str(path), True)

    def close(self):
        if self.proc.poll() is None:
            try:
                self.cmd('quit')
                self.proc.wait(timeout=20)
            finally:
                if self.proc.poll() is None:
                    self.proc.kill()
                    self.proc.wait()
        self.log.close()


def capture_game(repo, binary, game, out, scratch, probes=False):
    r = Run(repo, binary, repo / 'harness/testgames/build' / game, scratch)
    frames = []
    try:
        r.wait('boot', 0)
        for raw in (repo / PLANS[game]).read_text().splitlines():
            line = raw.strip()
            if not line or line.startswith('#'):
                continue
            if game == 'loop' and line == 'cmd advance 5':
                # The screen-loop plan intentionally never reaches five say statements.
                # Capture its held screen instead of asking the loop guard to time out.
                time.sleep(2)
                break
            op, _, arg = line.partition(' ')
            if op == 'cmd':
                if game == 'loop' and arg == 'jump screen_loop':
                    r.cmd('auto off', True)
                r.cmd(arg, arg.split()[0] not in ('start', 'jump', 'load'))
            elif op == 'wait':
                r.wait(arg, 0 if arg == 'menu True' else None)
            elif op == 'after_start':
                r.wait('say', 0)
            elif op == 'settle':
                time.sleep(float(arg))
            elif op == 'shot':
                checkpoint = arg.split()[0]
                # Movies and animated GIFs are deliberately not still-frame oracles.
                if 'volatile' not in arg and checkpoint != '12-gif-anim':
                    p = out / 'frames' / game / (checkpoint + '.png')
                    r.shot(p)
                    frames.append({'game': game, 'checkpoint': checkpoint, 'path': str(p.relative_to(out)),
                                   'sha256': digest(p), 'size': p.stat().st_size, 'renderer': 'wgpu-vulkan-lavapipe'})
            elif op == 'quit':
                break
            elif op != 'note':
                raise ValueError('unknown plan operation: ' + op)
        if game == 'loop':
            r.shot(out / 'frames/loop/screen-loop.png')
            p = out / 'frames/loop/screen-loop.png'
            frames.append({'game': game, 'checkpoint': 'screen-loop', 'path': str(p.relative_to(out)),
                           'sha256': digest(p), 'size': p.stat().st_size, 'renderer': 'wgpu-vulkan-lavapipe'})
        if probes:
            # A fresh namespace: the Ren'Py store replaces list, dict and str with revertable types.
            r.cmd("exec _legacy_probes = {}; exec(compile(open(%r).read(), 'legacy-probes', 'exec'), _legacy_probes)"
                  % str(HERE / 'probes.py'), True)
            for name in ('text', 'surface'):
                r.cmd("exec _legacy_probes['capture_%s'](%r)" % (name, str(out)), True)
        trace = [s for s in r.lines() if s.startswith(('say ', 'text ', 'label ', 'auto: choice ', 'advance-done '))]
        p = out / 'traces' / (game + '.txt')
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text('\n'.join(trace) + '\n')
    finally:
        r.close()
    return frames


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--repo', type=Path, required=True)
    ap.add_argument('--binary', type=Path, required=True)
    ap.add_argument('--out', type=Path, required=True)
    ap.add_argument('--scratch', type=Path, required=True)
    ap.add_argument('--games', default=','.join(PLANS))
    ap.add_argument('--probes', action='store_true')
    a = ap.parse_args()
    a.repo, a.binary, a.out, a.scratch = [p.resolve() for p in (a.repo, a.binary, a.out, a.scratch)]
    a.out.mkdir(parents=True, exist_ok=True)
    frames = []
    for game in a.games.split(','):
        print('Capturing ' + game, flush=True)
        scratch = a.scratch / game
        scratch.mkdir(parents=True, exist_ok=False)
        frames += capture_game(a.repo, a.binary, game, a.out, scratch, a.probes and game == 'text')
        dump(a.out / 'frames/index.json', frames)


if __name__ == '__main__':
    main()

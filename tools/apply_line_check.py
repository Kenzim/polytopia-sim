#!/usr/bin/env python3
"""Check `position apply` lines: the AI played one command per call (the line replayed each time, as the editor
does) must reach the same position, with the same commands, as one call that plays the whole turn.

usage: tools/apply_line_check.py <position.json>...   (make positions with `position from-save <file.state> out.json`)
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BIN = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[1] / "target")) / "release" / "examples" / "position"


def call(req: dict) -> dict:
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(req, f)
    try:
        r = subprocess.run([str(BIN), "apply", f.name], capture_output=True, text=True)
    finally:
        os.unlink(f.name)
    if r.returncode:
        raise SystemExit(r.stderr)
    return json.loads(r.stdout)


def main() -> int:
    bad = 0
    for name in sys.argv[1:]:
        pos = json.load(open(name))
        whole = call({"position": pos, "steps": [], "action": {"ai": "turn"}})
        base, steps, log = pos, [], []
        while True:
            r = call({"position": base, "steps": steps, "action": {"ai": "move"}})
            log += r["log"]
            if r["base"] is not None:
                base = r["base"]
            steps = r["steps"]
            for w in r["warnings"]:
                print("  warning:", w)
            if r["log"][-1]["kind"] == "EndTurn" or r["moves"]["over"]:
                break
        same = json.dumps(r["position"], sort_keys=True) == json.dumps(whole["position"], sort_keys=True)
        texts = [c["text"] for c in log] == [c["text"] for c in whole["log"]]
        bad += not (same and texts)
        nxt = whole["moves"]
        print(f"{name}: {len(whole['log'])} AI commands; one at a time gives the same position: {same}, same commands: {texts}")
        print(f"  then player {nxt['player']} ({'human' if nxt['human'] else 'AI'}), {len(nxt['commands'])} legal, AI would play {nxt['ai']['text']}")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())

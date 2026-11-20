#!/usr/bin/env python3
"""Turn the JSON lines of run.sh into the Markdown tables of docs/speed.md.

usage: python3 tools/bench_polybots/summarise.py results.jsonl

Every figure is the median over the rounds. A ratio is ours / theirs for the same kind of build, as the median of the
per-round ratios, with the lowest and highest round in brackets.
"""

import json
import statistics
import sys
from collections import defaultdict

BUILDS = ["theirs-release", "ours-release", "theirs-tuned", "ours-tuned"]
OPS = [
    ("new_game_us", "Start a new game (map generation included)", "{:,.0f}"),
    ("legal_us", "List the legal moves", "{:.2f}"),
    ("apply_us", "Apply one move", "{:.2f}"),
    ("clone_us", "Copy the game state", "{:.2f}"),
    ("legal_moves_avg", "Legal moves per position (count)", "{:.1f}"),
]


def median(xs):
    return statistics.median(xs) if xs else float("nan")


def ratio(per_round, ours, theirs):
    rs = [r[ours] / r[theirs] for r in per_round.values() if ours in r and theirs in r and r[theirs]]
    if not rs:
        return "n/a"
    return f"{median(rs):.2f} ({min(rs):.2f} to {max(rs):.2f})"


def main(path):
    host = None
    runs = defaultdict(lambda: defaultdict(dict))  # (size, threads, field) -> round -> build -> value
    ops = defaultdict(lambda: defaultdict(list))  # (size, build) -> field -> values
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if "host" in row:
                host = row["host"]
            elif "run" in row:
                r = row["run"]
                for field in ("dps", "dps_no_reset"):
                    runs[(r["size"], r["threads"], field)][row["round"]][row["build"]] = r[field]
            elif "ops" in row:
                o = row["ops"]
                for field, _, _ in OPS:
                    ops[(o["size"], row["build"])][field].append(o[field])

    if host:
        print(f"Host: {host['cpu']}, {host['logical_cpus']} logical CPUs, load average at the start {host['load_average']}.")
        print(f"{host['rustc']}; polytopia-sim {host['ours']}; polybots-engine {host['theirs'][:7]}; "
              f"rules file SHA-256 {host['rules_sha256'][:8]}...; {host['secs']} s per measurement.")
        print()

    for field, title in (("dps", "including new games"), ("dps_no_reset", "new-game time taken out")):
        print(f"Decisions per second, {title}:")
        print()
        print("| Board, threads | theirs release | ours release | ours / theirs | theirs tuned | ours tuned | ours / theirs |")
        print("| --- | ---: | ---: | ---: | ---: | ---: | ---: |")
        for size, threads, f_ in sorted(k for k in runs if k[2] == field):
            per_round = runs[(size, threads, f_)]
            med = {b: median([r[b] for r in per_round.values() if b in r]) for b in BUILDS}
            print(f"| {size} x {size}, {threads} | {med['theirs-release']:,.0f} | {med['ours-release']:,.0f} | "
                  f"{ratio(per_round, 'ours-release', 'theirs-release')} | {med['theirs-tuned']:,.0f} | "
                  f"{med['ours-tuned']:,.0f} | {ratio(per_round, 'ours-tuned', 'theirs-tuned')} |")
        print()

    for size in sorted({k[0] for k in ops}):
        print(f"Per operation, {size} x {size}, one thread (microseconds):")
        print()
        print("| Operation | theirs release | ours release | theirs tuned | ours tuned |")
        print("| --- | ---: | ---: | ---: | ---: |")
        for field, label, fmt in OPS:
            cells = [fmt.format(median(ops[(size, b)][field])) for b in BUILDS]
            print(f"| {label} | " + " | ".join(cells) + " |")
        print()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])

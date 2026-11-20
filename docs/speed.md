# Speed: polytopia-sim and polybots-engine

[polybots-engine](https://github.com/polybots-dev/polybots-engine) (crate `engine-fast`, Apache-2.0) is another
engine for The Battle of Polytopia, built for training bots at high throughput. This page compares the two on the
same workload, measured on 18 November 2026. None of polybots-engine's code is in this repository: the harness in
`tools/bench_polybots/` is written against its public API, and Cargo fetches it from GitHub at the commit measured.

## Summary

On random play, polytopia-sim makes **about 1.5 to 1.6 times as many decisions per second** as polybots-engine
when both are built the same way: as each project ships its release build, and when both get the same tuning (full
link-time optimisation, mimalloc and profile-guided optimisation). The ratio holds from one thread to 48. Tuning
speeds both engines up by about a quarter, so comparing our tuned build with their plain one would show about 2
times; that comparison is not like for like.

Ours lists legal moves about 1.9 times faster, applies a move 1.1 to 1.6 times faster and sets up a new game 15 to
20% faster. **polybots-engine copies a game state about twice as fast** (1.0 against 1.9 microseconds on 11 x 11, 1.6
against 3.4 on 16 x 16, each engine's best build), which matters for search that copies the state at every node.
It also covers more of the game in places, and offers things this engine does not (see [Scope](#scope)).

## Results

Decisions per second; a decision is one command by one player. Medians of three rounds. A ratio is ours divided by
theirs for the same kind of build, the median of the three rounds' ratios, with the lowest and highest round in
brackets.

Including the time to set up new games:

| Board, threads | theirs release | ours release | ours / theirs | theirs tuned | ours tuned | ours / theirs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 11 x 11, 1 | 52,121 | 80,269 | 1.55 (1.53 to 1.55) | 66,314 | 103,980 | 1.55 (1.53 to 1.57) |
| 11 x 11, 8 | 412,656 | 636,037 | 1.54 (1.54 to 1.55) | 534,492 | 790,337 | 1.48 (1.47 to 1.49) |
| 11 x 11, 48 | 1,997,371 | 3,020,033 | 1.52 (1.44 to 1.53) | 2,628,139 | 3,828,428 | 1.50 (1.45 to 1.51) |
| 16 x 16, 1 | 37,561 | 60,284 | 1.60 (1.60 to 1.66) | 46,863 | 74,999 | 1.61 (1.54 to 1.62) |
| 16 x 16, 8 | 298,404 | 473,691 | 1.59 (1.58 to 1.60) | 368,038 | 565,320 | 1.53 (1.53 to 1.54) |
| 16 x 16, 48 | 1,442,228 | 2,302,303 | 1.59 (1.57 to 1.63) | 1,747,684 | 2,829,444 | 1.59 (1.54 to 1.65) |

With the time spent setting up new games taken out:

| Board, threads | theirs release | ours release | ours / theirs | theirs tuned | ours tuned | ours / theirs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 11 x 11, 1 | 60,469 | 97,355 | 1.62 (1.60 to 1.63) | 79,043 | 131,207 | 1.64 (1.62 to 1.66) |
| 11 x 11, 8 | 478,620 | 772,196 | 1.61 (1.61 to 1.62) | 639,779 | 993,526 | 1.55 (1.54 to 1.57) |
| 11 x 11, 48 | 2,306,229 | 3,644,887 | 1.59 (1.50 to 1.60) | 3,135,715 | 4,768,627 | 1.56 (1.51 to 1.58) |
| 16 x 16, 1 | 46,535 | 81,207 | 1.74 (1.74 to 1.81) | 60,047 | 106,676 | 1.79 (1.71 to 1.80) |
| 16 x 16, 8 | 369,422 | 631,826 | 1.72 (1.70 to 1.72) | 470,313 | 791,377 | 1.67 (1.67 to 1.68) |
| 16 x 16, 48 | 1,772,036 | 3,043,099 | 1.71 (1.69 to 1.75) | 2,220,653 | 3,930,025 | 1.73 (1.68 to 1.80) |

Per operation, one thread, in microseconds (each operation timed on its own over whole games):

| Operation, 11 x 11 | theirs release | ours release | theirs tuned | ours tuned |
| --- | ---: | ---: | ---: | ---: |
| Set up a new game (map generation included) | 2,652 | 2,196 | 2,417 | 1,968 |
| List the legal moves | 12.21 | 6.48 | 8.40 | 4.54 |
| Apply one move | 4.53 | 4.03 | 3.96 | 3.24 |
| Copy the game state | 0.99 | 1.94 | 1.75 | 1.88 |
| Legal moves per position (count) | 44.4 | 44.9 | 44.5 | 45.2 |

| Operation, 16 x 16 | theirs release | ours release | theirs tuned | ours tuned |
| --- | ---: | ---: | ---: | ---: |
| Set up a new game (map generation included) | 5,057 | 4,257 | 4,699 | 3,996 |
| List the legal moves | 14.76 | 7.57 | 10.30 | 5.30 |
| Apply one move | 7.35 | 5.09 | 6.79 | 4.25 |
| Copy the game state | 1.60 | 3.41 | 2.26 | 3.41 |
| Legal moves per position (count) | 57.6 | 62.0 | 57.3 | 62.2 |

The raw measurements are in [speed-results.jsonl](speed-results.jsonl); `tools/bench_polybots/summarise.py` turns
them into the tables above.

## How it was measured

**Versions.** polybots-engine at commit `e2e4029` (`e2e4029fda2e5d870dbc2dc847ed421dd0dc44cf`). polytopia-sim at
the engine code of this release. Both built with Rust 1.92.0 (polybots-engine needs 1.92 or later).

**Rules data.** polybots-engine reads the game's own rules file, which it does not ship: its
`tools/extract_rules.py` copies it out of an installed copy of the game (version 2.18, game version 124; SHA-256
`c6806c77963b5377bdbc2570bf0a3abece622e4e148883754dd190b02f890086`, which it checks). The file was extracted
locally and is not, and never will be, in this repository. polytopia-sim uses its own rules data for game version
124 (`data/rules/`).

**Games.** The same on both sides:

- 1v1, Imperius against Imperius, Might mode (the game ends when one player holds every capital), Lakes map type,
  game version 124; board 11 x 11 (`MapSize::Tiny`) or 16 x 16 (`MapSize::Normal`). The harness plays both seats.
- Policy: End Turn one time in ten, otherwise a uniformly random pick among the other legal moves. This is the
  policy of polybots-engine's own CPU benchmark.
- **No Resign.** polytopia-sim has no Resign command, and polybots-engine's is left out of the random pick; with it
  in, games last about four decisions and the run measures game set-up instead.
- Each game stops after 1,000 decisions, or earlier if someone wins, and a new game with the next seed takes its
  place. About 99% of games reach the cap on both sides (994 to 1,000 decisions per game on average).
- Each engine through its plain single-game interface: polybots-engine's `reset`, `menu` and `step::advance`, not
  its batch interface; polytopia-sim's `Engine::new_game`, `legal_commands` and `apply_unchecked`. Neither builds a
  player's view or observation in this loop.
- The seeds are the same, but the starting maps are not always identical: in a check of seeds 1 to 200, both
  engines generated the same map for 180 (11 x 11) and 181 (16 x 16) of them. The rest differ entirely, which
  points at a different choice in the game's search for a fair map rather than in the map generator itself. The
  harnesses' `maps` mode prints every starting map for this comparison.

**Builds.** Two per engine:

- *release*: each project's own release build. polybots-engine: the release profile it ships for its batch and
  Python packages (thin link-time optimisation, one code generation unit, overflow checks on). polytopia-sim:
  `cargo build --release` with Cargo's defaults (no cross-crate link-time optimisation, 16 code generation units,
  overflow checks off), the system allocator.
- *tuned*: the same for both: full link-time optimisation, one code generation unit, overflow checks off, mimalloc
  as the allocator, and profile-guided optimisation trained for 15 seconds per board size on one thread of this same
  random play. Training on the measured workload flatters both builds equally. polybots-engine's tuned build was
  also tried without mimalloc: it was the same or up to 2% slower, so mimalloc does not handicap it.

**Machine.** Two AMD EPYC 7443 processors (48 cores, 96 hardware threads), 503 GB of memory, Debian 13. The machine
was shared: other work kept the load average at 8 to 10 before the runs. Every run was at `nice -n 10`, threads not
pinned; 48 threads is one per physical core.

**Runs.** 10 seconds per measurement. Three rounds; within a round the four builds take turns for each board size
and thread count, so slow periods on the machine fall on all builds alike. One build's rounds differed by up to 6%
(at 48 threads), and the ratios by less (see the brackets).

**What this does not show.** Random play visits mostly early-game positions with few units; long games and AI-driven
play may give other ratios. On 16 x 16 ours lists about 62 legal moves per position against their 58: some moves
are counted or split differently, which was not traced, so the per-decision cost compares slightly different
lists. polybots-engine's batch interface, its per-player view and its CUDA backend are not measured here.

## Reproduce it

You need your own copy of the game (version 2.18) for the rules file, and about 20 minutes on a machine like the one
above (less with fewer threads).

```sh
rustup toolchain install 1.92.0 --profile minimal --component llvm-tools

# The rules file, with polybots-engine's own tool.
git clone https://github.com/polybots-dev/polybots-engine
cd polybots-engine && git checkout e2e4029
python3 -m venv .venv && . .venv/bin/activate && pip install UnityPy==1.25.4
python3 tools/extract_rules.py "<the game's install folder>" rules.json
cd -

# From this repository.
RULES=/path/to/rules.json tools/bench_polybots/run.sh > results.jsonl
python3 tools/bench_polybots/summarise.py results.jsonl
```

`run.sh` builds both engines in both builds (Cargo fetches polybots-engine at the measured commit), trains the
profile-guided builds, then measures. `THREADS`, `SIZES`, `SECS` and `ROUNDS` change what it runs; the first lines
of the script list them. The binaries also run on their own: `bench-ours ops 11 10` or
`bench-theirs rules.json run 16 8 10`.

## Scope

Speed is one difference; these are the others that matter when choosing between them.

| | polytopia-sim | polybots-engine |
| --- | --- | --- |
| Game versions | 51 to 124 load and play; 121 to 123 checked against recorded games | 124 (its recorded games are from 123, which has the same rules data) |
| Tribes | all, Elyrion and Aquarion included | all, Elyrion and Aquarion included |
| Players | up to 9 on 11 x 11 and 16 on 16 x 16 | 2 to 4 |
| The game's built-in AI | yes | no |
| Saves from the game | reads versions 121 to 124, replays them | its own checkpoint format |
| Player views for learning | no (the fog of war is kept per player; building a view is up to the caller) | yes, checked not to leak hidden information |
| Many games per call, GPU | no | a batch interface, a numpy package and a CUDA backend |
| Python | a package with one game per object | a numpy package built around the batch |
| Checked against the game | 44 recorded games replayed command for command, ten maps tile for tile (the recorded games are not published) | 1,097 recorded online games replayed, according to its README (not published) |

"""The engine_fast batch interface: the same names, shapes and rules as polybots-engine's package."""
import numpy as np
import pytest

from polytopia_sim import engine_fast as ef

EXPLORED = ef.TILE_PLANES.index("explored")


def started(games=4, size=11, tribes=(7, 7), workers=1, outputs=(), **kw):
    b = ef.Batch(games=games, size=size, workers=workers, outputs=outputs)
    b.reset(np.arange(games), np.arange(1, games + 1), list(tribes), **kw)
    return b


def play(batch, steps, seed=0):
    """Random play; returns every observation along the way."""
    rng = np.random.default_rng(seed)
    seen = []
    for _ in range(steps):
        offsets, rows = batch.moves()
        tiles, scalars, players, tech = batch.views()
        seen.append((batch.status(), rows, tiles, scalars, players, tech))
        pick = np.full(len(batch), ef.SKIP, np.int64)
        for g in np.flatnonzero(batch.status() == 1):
            n = offsets[g + 1] - offsets[g]
            kept = [i for i in range(n) if rows[offsets[g] + i, 0] != ef.KINDS["Resign"]]
            pick[g] = kept[rng.integers(len(kept))]
        batch.step(pick)
    return seen


def test_names_and_shapes():
    assert ef.MOVE_FIELDS == ["kind", "source", "target", "option", "opponent", "price"]
    assert len(ef.TILE_PLANES) == 29 and ef.STATUS[1] == "playing" and ef.SKIP == -1
    b = started(outputs=ef.OUTPUTS)
    assert len(b) == 4 and b.size == 11
    assert b.status().tolist() == [1, 1, 1, 1]
    offsets, rows = b.moves()
    assert offsets.shape == (5,) and rows.shape == (offsets[-1], 6) and rows.dtype == np.int32
    tiles, scalars, players, tech = b.views()
    assert tiles.shape == (4, 29, 121) and tiles.dtype == np.int16
    assert scalars.shape == (4, 3) and players.shape == (4, 4, 4)
    assert b.previews().shape == (offsets[-1], 4)
    assert b.defence().shape == (4, 121) and b.max_health().shape == (4, 121)
    assert b.income().shape == (4,)


def test_same_seed_same_game_whatever_the_workers():
    one, many = play(started(workers=1), 60), play(started(workers=4), 60)
    for a, b in zip(one, many):
        for x, y in zip(a, b):
            np.testing.assert_array_equal(x, y)


def test_rows_by_index_skip_and_rejection():
    b = started(games=3)
    offsets, _ = b.moves()
    applied = b.step(np.array([offsets[1] - offsets[0], 0, ef.SKIP]))
    assert applied.tolist() == [2, 1, 0]


def test_no_row_names_an_unexplored_tile():
    b = started(games=8, tribes=(7, 9))
    for _ in range(80):
        offsets, rows = b.moves()
        explored = b.views()[0][:, EXPLORED]
        for g in np.flatnonzero(b.status() == 1):
            for r in rows[offsets[g]:offsets[g + 1]]:
                anchor = r[1] if r[1] >= 0 else r[2]
                assert anchor < 0 or explored[g, anchor] == 1, r
        play(b, 1)


@pytest.mark.parametrize("size", [8, 12, 30])
def test_sides_between_and_below_the_game_sizes(size):
    b = started(games=2, size=size, tribes=(7, 7, 7, 7) if size != 8 else (7, 7))
    assert b.status().tolist() == [1, 1]
    assert b.views()[0].shape[2] == size * size


def test_resigning_ends_with_the_other_seat_winning():
    b = started(games=1)
    offsets, rows = b.moves()
    resign = int(np.flatnonzero(rows[:, 0] == ef.KINDS["Resign"])[0])
    mover = b.views()[1][0, 0]
    b.step(np.array([resign]))
    assert b.status().tolist() == [2] and b.winner().tolist() == [3 - mover]


def test_refusals():
    with pytest.raises(ValueError, match="nano"):
        ef.Batch(games=1, size=10)
    with pytest.raises(ValueError, match="unknown output"):
        ef.Batch(games=1, outputs=("health",))
    with pytest.raises(ValueError, match="cpu backend only"):
        ef.Batch(games=1, backend="gpu", outputs=("income",))
    with pytest.raises(ValueError, match="gpu backend"):
        ef.Batch(games=1, backend="gpu")
    b = started(games=1)
    with pytest.raises(ValueError, match="max_health"):
        b.max_health()
    b.reset(np.arange(1), np.arange(1, 2), [7])
    assert b.status().tolist() == [4] and b.reason(0)

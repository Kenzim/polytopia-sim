import copy
import json
import os

import pytest

import polytopia_sim as ps


def tiny(**kw):
    return ps.Game([("imperius", True), ("bardur", False)], seed=11, size="tiny", **kw)


def test_new_game_and_moves():
    g = tiny(mode="might")
    assert g.turn == 0 and g.current_player == 1 and not g.is_over
    assert g.game_version == ps.GAME_VERSION
    moves = g.legal_moves()
    assert "EndTurn" in moves
    info = g.legal_moves_info()
    assert [m["text"] for m in info] == moves
    g.play(moves[0])
    with pytest.raises(ValueError):
        g.play("Move { unit: 9999, to: WorldCoordinates { x: 0, y: 0 } }")
    assert dict(g.tribes()) == {1: "imperius", 2: "bardur"}


def test_bad_settings():
    with pytest.raises(ValueError):
        ps.Game([("atlantis", True)])
    with pytest.raises(ValueError):
        tiny(map="moon")
    with pytest.raises(ValueError):
        tiny(version=50)


def test_ai_and_copies():
    g = tiny(turn_limit=5)
    before = g.to_position()
    c = g.copy()
    d = copy.deepcopy(g)
    g.play("EndTurn")
    assert c.to_position() == before and d.to_position() == before
    assert isinstance(g.ai_move(), str)
    assert g.step_ai() is not None
    g.play_out()
    assert g.is_over
    assert len(g.scores()) == 2 and g.winners()


def test_positions_round_trip(tmp_path):
    g = tiny()
    text = g.to_position()
    assert json.loads(text)["format"] == "polytopia-position"
    back = ps.Game.from_position(text)
    assert back.legal_moves() == g.legal_moves()
    path = tmp_path / "game.json"
    path.write_text(text)
    assert ps.Game.open(str(path)).legal_moves() == g.legal_moves()


def test_quick_maps():
    assert tiny(map_attempts=1).to_position() != tiny().to_position()


@pytest.mark.skipif(not os.environ.get("POLYTOPIA_REGRESS"), reason="POLYTOPIA_REGRESS not set")
def test_replay_a_regression_game():
    d = os.environ["POLYTOPIA_REGRESS"]
    name = sorted(f for f in os.listdir(d) if f.endswith(".state"))[-1]
    r = ps.replay(os.path.join(d, name), check_ai=True)
    assert r["illegal"] == [] and r["differences"] == []
    assert r["commands"] > 0 and isinstance(r["game"], ps.Game)

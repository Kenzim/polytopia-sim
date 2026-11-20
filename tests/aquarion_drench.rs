//! The aquapult's Drench: an attack floods the target tile.
//!
//! A unit with Drench can target any floodable tile in range, a unit on it or not. With no unit there the attack
//! turns the unit to the tile, floods it (and spends the unit), as long as the tile is floodable: land that is not a
//! mountain and not flooded yet. A unit on the tile is attacked as usual and the tile is flooded as well.
//!
//! Evidence from the games: retail's attack action queues the flood for a Drench attacker after the unit-on-tile
//! branches too (corpus games c234bae1 and fe8b64e2: the server has the target of a killing aquapult attack flooded).
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const AQUAPULT: i64 = 55;
const WARRIOR: i64 = 2;
const FLOODED: u16 = Engine::EFFECT_FLOODED;
const SWAMPED: u16 = Engine::EFFECT_SWAMPED;

fn wc(x: i32, y: i32) -> Wc {
    Wc { x, y }
}

/// A 9x3 board of fields with water at (3,1); player 1 (Aquarion) has an aquapult on the water, player 2 is Bardur.
/// `tile` adds json (or `terrain:N`) to single tiles and `enemy` puts a warrior of player 2 on a tile.
fn board(tile: &[(i32, i32, &str)], enemy: Option<(i32, i32)>) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..9 {
            let terrain = tile
                .iter()
                .find(|e| (e.0, e.1) == (x, y) && e.2.starts_with("terrain:"))
                .map_or(if (x, y) == (3, 1) { 1 } else { 3 }, |e| e.2[8..].parse::<i32>().unwrap());
            let mut t = format!(r#""x":{x},"y":{y},"terrain":{terrain},"climate":3,"explored":[1,2]"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,2]"#),
                _ => {}
            }
            for (tx, ty, extra) in tile {
                if (*tx, *ty) == (x, y) && !extra.starts_with("terrain:") {
                    t.push(',');
                    t.push_str(extra);
                }
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let mut units = format!(r#"{{"id":1,"type":{AQUAPULT},"owner":1,"x":3,"y":1,"health":100}}"#);
    if let Some((x, y)) = enemy {
        units.push_str(&format!(r#",{{"id":2,"type":{WARRIOR},"owner":2,"x":{x},"y":{y},"health":100}}"#));
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":3,"human":true,"stars":5,"tech":[0,45],"skin":17}},{{"id":2,"tribe":4,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":2,"level":1}}],
           "units":[{units}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn attack(target: Wc) -> Command {
    Command::Attack { unit: 1, target }
}

#[test]
fn an_attack_on_an_empty_field_floods_it_and_spends_the_unit() {
    let mut e = board(&[], None);
    assert!(e.legal_commands().contains(&attack(wc(5, 1))), "an empty field in range is a target");
    e.apply(attack(wc(5, 1))).expect("the attack is legal");
    assert!(e.has_effect(wc(5, 1), FLOODED));
    assert!(e.has_effect(wc(5, 1), SWAMPED), "with the swamp skin's effect");
    let u = e.unit(1).unwrap();
    assert!(u.attacked && u.moved, "the unit is spent");
    assert!(!e.legal_commands().iter().any(|c| matches!(c, Command::Attack { .. })));
}

#[test]
fn a_forest_floods_too() {
    let mut e = board(&[(5, 1, "terrain:5")], None);
    e.apply(attack(wc(5, 1))).expect("the attack is legal");
    assert!(e.has_effect(wc(5, 1), FLOODED));
}

#[test]
fn mountains_water_and_flooded_tiles_are_not_targets() {
    let e = board(&[(5, 1, "terrain:4"), (4, 0, "terrain:1"), (4, 2, r#""effects":[1]"#)], None);
    let attacks: Vec<_> = e.legal_commands().into_iter().filter(|c| matches!(c, Command::Attack { .. })).collect();
    for t in [wc(5, 1), wc(4, 0), wc(4, 2)] {
        assert!(!attacks.contains(&attack(t)), "{t:?} cannot be drenched");
    }
    assert!(attacks.contains(&attack(wc(6, 1))));
}

#[test]
fn an_enemy_on_the_tile_is_attacked_and_the_tile_floods_too() {
    let mut e = board(&[], Some((5, 1)));
    e.apply(attack(wc(5, 1))).expect("the attack is legal");
    assert!(e.unit(2).is_none_or(|u| u.health < 100), "the warrior was hit");
    assert!(e.has_effect(wc(5, 1), FLOODED));
}

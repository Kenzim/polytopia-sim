//! Two facings that the save format keeps and the engine used to get wrong (found in the corpus final-state comparison).
//!
//! An infiltration spawns each unit with a train action that carries the infiltrated city as its look coordinates
//! (the train action turns the new unit to them), so every spawned unit faces the city, or faces nowhere (8)
//! when it stands on the city's own tile. A ranged attack with splash turns the attacker towards each splash victim in
//! turn; the splash actions run last in, first out, so the attacker ends up facing the first victim in neighbour order.
use polytopia_sim::engine::{Command, Engine, GameOptions};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const CITY: i32 = 1;
const FIELD: i32 = 3;
const CYMANTI: i64 = 17;
const BARDUR: i64 = 3;
const FIREDRAGON: i64 = 19;
const WARRIOR: i64 = 2;

fn type_of(e: &Engine, id: &str) -> i32 {
    (0..200).find(|i| e.t.unit(*i).is_some_and(|u| u.data.id == id)).unwrap_or_else(|| panic!("unit type {id}"))
}

#[test]
fn infiltration_spawns_face_the_city() {
    let mut e = Engine::new_game(&GameOptions::default());
    let city =
        e.s.map.tiles.iter().find(|t| t.owner == 2 && t.improvement == CITY).expect("a city of player 2").coordinates;
    let beside = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
    e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
    let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == beside).unwrap();
    t.terrain = FIELD;
    t.improvement = 0;
    t.has_ruin = false;
    let cloak = type_of(&e, "cloak");
    let id = e.spawn_unit(1, cloak, beside, beside, false).expect("the cloak");
    e.apply(Command::Attack { unit: id, target: city }).expect("the infiltration is legal");
    let spawned: Vec<_> = e.s.units.iter().filter(|u| u.owner == 1 && u.id > id).cloned().collect();
    assert!(!spawned.is_empty(), "the infiltration spawned units");
    for u in spawned {
        assert_eq!(
            u.direction,
            Engine::direction_of(u.coordinates, city),
            "unit {} at {:?} faces the city {city:?}",
            u.id,
            u.coordinates
        );
    }
}

#[test]
fn a_splash_attacker_ends_facing_the_first_victim_in_neighbour_order() {
    let mut tiles = Vec::new();
    for y in 0..9 {
        for x in 0..9 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (8, 8) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[8,8]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":9,"height":9,"current_player":1}},
           "players":[{{"id":1,"tribe":{CYMANTI},"human":true,"stars":5}},{{"id":2,"tribe":{BARDUR},"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":1}},{{"x":8,"y":8,"level":1}}],
           "units":[{{"id":1,"type":{FIREDRAGON},"owner":1,"x":2,"y":4,"health":100}},
                    {{"id":2,"type":{WARRIOR},"owner":2,"x":4,"y":4,"health":100}},
                    {{"id":3,"type":{WARRIOR},"owner":2,"x":4,"y":5,"health":100}},
                    {{"id":4,"type":{WARRIOR},"owner":2,"x":4,"y":3,"health":100}}]}}"#,
        tiles.join(",")
    );
    let mut e = Engine::from_position(&Json::parse(&text).unwrap()).unwrap();
    let target = Wc { x: 4, y: 4 };
    let first = e.neighbors(target).into_iter().find(|n| e.unit_at(*n).is_some()).expect("a splash victim");
    e.apply(Command::Attack { unit: 1, target }).expect("the attack is legal");
    assert!(
        e.unit(3).is_some_and(|u| u.health < 100) && e.unit(4).is_some_and(|u| u.health < 100),
        "both neighbours took splash damage"
    );
    let from = Wc { x: 2, y: 4 };
    assert_eq!(
        e.unit(1).map(|u| u.direction),
        Some(Engine::direction_of(from, first)),
        "faces the first victim {first:?}"
    );
    assert_ne!(
        Engine::direction_of(from, Wc { x: 4, y: 5 }),
        Engine::direction_of(from, Wc { x: 4, y: 3 }),
        "the two victims lie in different directions"
    );
}

#[test]
fn a_poison_infiltration_egg_is_homed_in_the_infiltrated_city_and_a_dagger_has_no_home() {
    for (infiltrator, egg) in [(61, true), (type_of(&Engine::new_game(&GameOptions::default()), "cloak"), false)] {
        let mut e = Engine::new_game(&GameOptions::default());
        let city =
            e.s.map
                .tiles
                .iter()
                .find(|t| t.owner == 2 && t.improvement == CITY)
                .expect("a city of player 2")
                .coordinates;
        let beside = Wc { x: if city.x > 0 { city.x - 1 } else { city.x + 1 }, y: city.y };
        e.s.units.retain(|u| u.coordinates != city && u.coordinates != beside);
        let t = e.s.map.tiles.iter_mut().find(|t| t.coordinates == beside).unwrap();
        t.terrain = FIELD;
        t.improvement = 0;
        t.has_ruin = false;
        let id = e.spawn_unit(1, infiltrator, beside, beside, false).expect("the infiltrator");
        e.apply(Command::Attack { unit: id, target: city }).expect("the infiltration is legal");
        let spawned: Vec<_> = e.s.units.iter().filter(|u| u.owner == 1 && u.id > id).cloned().collect();
        assert!(!spawned.is_empty());
        for u in spawned {
            if egg {
                assert_eq!(u.type_idx, 60);
                assert_eq!(
                    u.home, city,
                    "an egg is homed in the infiltrated city whose tile it stands on, not (-1, -1)"
                );
            } else {
                assert_eq!(u.home, Wc { x: -1, y: -1 }, "an independent dagger keeps no home");
            }
        }
    }
}

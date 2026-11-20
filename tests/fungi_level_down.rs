//! A Fungi's top level is one higher for a player with the Microbes ability (the Recycling tech). When its level is above
//! what its owner allows (the tile changed hands in a capture), the owner's next turn start takes a level off it, and
//! retail's level-down action also queues a population decrease on the ruling city for each point of
//! population the growth rewards gave. The engine dropped the level only, so the city kept the population: in recorded
//! Cymanti games a captured city ends one population (and a level once the next level-up came) above the server's.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::WorldCoordinates as Wc;

const FUNGI: i32 = 37;
const RECYCLING: i64 = 36;
const CITY: Wc = Wc { x: 1, y: 1 };
const FUNGUS: Wc = Wc { x: 2, y: 1 };

/// Player 2 to move; player 1 owns a city with a level 2 Fungi in its area and has `techs`.
fn board(techs: &str) -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..8 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (1, 1) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[1,1]"#),
                (2, 1) => t.push_str(&format!(r#","owner":1,"ruling":[1,1],"improvement":{FUNGI}"#)),
                (7, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[7,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":6,"width":8,"height":3,"current_player":2}},
           "players":[{{"id":1,"tribe":17,"human":true,"stars":5,"tech":[{techs}]}},{{"id":2,"tribe":3,"human":true,"stars":5}}],
           "tiles":[{}],
           "improvements":[{{"x":1,"y":1,"level":2,"population":6,"xp":0}},{{"x":7,"y":2,"level":1}},{{"x":2,"y":1,"level":2,"founded":0}}],
           "units":[]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn city_pop_and_fungi_level(e: &Engine) -> (i32, u16) {
    (i32::from(e.improvement_rt(CITY).unwrap().population), e.improvement_rt(FUNGUS).unwrap().level)
}

#[test]
fn a_level_lost_at_turn_start_takes_the_population_it_gave() {
    let mut e = board("");
    let (pop, level) = city_pop_and_fungi_level(&e);
    assert_eq!(level, 2);
    e.apply(Command::EndTurn).unwrap();
    assert_eq!(e.current_player(), 1);
    assert_eq!(city_pop_and_fungi_level(&e), (pop - 1, 1));
}

#[test]
fn a_fungi_within_its_cap_keeps_both() {
    let mut e = board(&RECYCLING.to_string());
    let (pop, level) = city_pop_and_fungi_level(&e);
    e.apply(Command::EndTurn).unwrap();
    assert_eq!(city_pop_and_fungi_level(&e), (pop, level));
}

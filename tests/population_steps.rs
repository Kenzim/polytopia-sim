//! A population gain reaches a city one point at a time, and each point checks the level-up.
//!
//! Retail's population action adds a single point, then queues one city level-up when the city has enough xp and the
//! level-up action levels the city once. A gain of several points therefore raises a level-up reward as soon as the
//! point that earns it, and the points still to come wait under that reward and run when it is answered, which can
//! raise a second reward at once. The engine used to add the whole gain first and level the city up only once: a city
//! with level 2 and 1 xp that gained 6 stayed at level 3 with 4 xp, without the server's second reward.
use polytopia_sim::engine::{Command, Engine};
use polytopia_sim::json::Json;
use polytopia_sim::{CityReward, WorldCoordinates as Wc};

const CAPITAL: Wc = Wc { x: 0, y: 0 };

/// Player 1's capital at level 2 with 1 xp (it levels up above 2), and a capital of player 2 far away.
fn board() -> Engine {
    let mut tiles = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let mut t = format!(r#""x":{x},"y":{y},"terrain":3,"climate":2"#);
            match (x, y) {
                (0, 0) => t.push_str(r#","city":true,"capital":true,"owner":1,"ruling":[0,0]"#),
                (4, 2) => t.push_str(r#","city":true,"capital":true,"owner":2,"ruling":[4,2]"#),
                _ => {}
            }
            tiles.push(format!("{{{t}}}"));
        }
    }
    let text = format!(
        r#"{{"format":"polytopia-position","game":{{"turn":5,"width":5,"height":3,"current_player":1}},
           "players":[{{"id":1,"tribe":2,"human":true,"stars":20}},{{"id":2,"tribe":3}}],
           "tiles":[{}],
           "improvements":[{{"x":0,"y":0,"level":2,"xp":1,"population":1}},{{"x":4,"y":2,"level":1}}]}}"#,
        tiles.join(",")
    );
    Engine::from_position(&Json::parse(&text).unwrap()).unwrap()
}

fn capital(e: &Engine) -> (u16, i16, i16, usize) {
    let tile = e.tile(CAPITAL).unwrap().index;
    let r = e.s.improvements.iter().find(|r| r.tile == tile).unwrap();
    (r.level, r.population, r.xp, r.rewards.len())
}

#[test]
fn a_gain_that_spans_two_levels_raises_the_second_reward_after_the_first_is_answered() {
    let mut e = board();
    e.add_population(CAPITAL, 6);
    // The second point earns level 3; the four points still to come wait under its reward.
    assert_eq!(capital(&e), (3, 3, 0, 0), "level, population, xp, rewards");
    assert_eq!(e.triggers.len(), 1);

    e.apply(Command::CityReward { city: CAPITAL, reward: CityReward::Resources }).unwrap();
    // The held points run: the fourth of them earns level 4 and raises its reward.
    assert_eq!(capital(&e), (4, 7, 0, 1));
    assert_eq!(e.triggers.len(), 1, "the second reward is pending");

    e.apply(Command::CityReward { city: CAPITAL, reward: CityReward::BorderGrowth }).unwrap();
    assert_eq!(capital(&e), (4, 7, 0, 2));
    assert!(e.triggers.is_empty());
}

#[test]
fn a_gain_that_stays_below_the_next_level_adds_every_point() {
    let mut e = board();
    e.add_population(CAPITAL, 1);
    e.add_population(CAPITAL, 0);
    assert_eq!(capital(&e), (2, 2, 2, 0));
    assert!(e.triggers.is_empty());
}

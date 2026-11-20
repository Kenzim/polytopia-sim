//! A border growth reward can cut another player's route: a route may not cross foreign territory. The game re-runs
//! the route pass and every player's city connections straight after the reward, so a city whose only route ran
//! over the newly claimed tiles is disconnected at once (it and its capital each lose one population).
//!
//! The replay test reads two live games where this happened (50d73fc9 turn 14, 25bf9c82 turn 22) from
//! `POLYTOPIA_DIVERGENCE`, and skips without them.
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::Engine;

fn divergence_dir() -> String {
    std::env::var("POLYTOPIA_DIVERGENCE").unwrap_or_default()
}

/// (game, the border growth command, the city it disconnects, that city's tile index)
const GAMES: [(&str, usize, u8, i32); 2] = [
    ("50d73fc9-b523-480c-39aa-08df30e80ec3-1792937560", 257, 2, 28),
    ("25bf9c82-ca3e-4fb3-3b75-08df30e80ec3-1792939091", 473, 1, 106),
];

fn replay(save: &SaveFile, stop_after: Option<usize>) -> Engine {
    let mut e = Engine::from_state(save.initial.clone());
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    for (n, c) in save.commands.iter().enumerate() {
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        if c.kind == 22 {
            if e.end_match().is_err() {
                e.s.current_command += 1;
            }
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            e.s.current_command += 1;
            continue;
        };
        e.apply_unchecked(cmd);
        if stop_after == Some(n) {
            break;
        }
    }
    e
}

#[test]
fn border_growth_disconnects_a_city_whose_route_it_cuts() {
    let dir = divergence_dir();
    let mut checked = 0;
    for (game, command, player, tile) in GAMES {
        let (Ok(init), Ok(cur)) =
            (std::fs::read(format!("{dir}/{game}_init.bin")), std::fs::read(format!("{dir}/{game}_cur.bin")))
        else {
            eprintln!("skipped {game}: blobs absent in {dir}");
            continue;
        };
        let save = SaveFile::parse_server(&init, &cur).unwrap();
        let c = &save.commands[command];
        assert_eq!((c.kind, c.n), (11, 5), "{game}: command {command} is a border growth reward");

        // Straight after the reward the city is no longer connected.
        let e = replay(&save, Some(command));
        let rt = e.s.improvements.iter().find(|r| r.tile == tile).expect("the city");
        assert_eq!(rt.connected_to_capital_of_player, 0, "{game}: city at tile {tile} is disconnected by the reward");
        assert!(!e.find_connected_cities(player).iter().any(|w| e.tile_index(*w) == Some(tile as usize)));

        // At the end of the stack the players and cities equal the server's.
        let e = replay(&save, None);
        for theirs in save.current.players.iter().filter(|p| p.id != polytopia_sim::NATURE) {
            let ours = e.s.players.iter().find(|p| p.id == theirs.id).unwrap();
            assert_eq!(
                (ours.currency, ours.score),
                (theirs.currency, theirs.score),
                "{game}: player {} (stars, score)",
                theirs.id
            );
        }
        for theirs in &save.current.improvements {
            let Some(ours) = e.s.improvements.iter().find(|r| r.tile == theirs.tile) else { continue };
            assert_eq!(
                (ours.level, ours.population, ours.xp, ours.connected_to_capital_of_player),
                (theirs.level, theirs.population, theirs.xp, theirs.connected_to_capital_of_player),
                "{game}: improvement at tile {} (level, population, xp, connected)",
                theirs.tile
            );
        }
        checked += 1;
    }
    eprintln!("{checked} games checked");
}

//! Replay a retail save's command stack on the engine and report where it diverges.
//!
//! ```text
//! cargo run --release --example replay -- <file.state> [ai]   # `ai` also compares each recorded AI move
//! ```
use polytopia_sim::engine::replay::SaveFile;
use polytopia_sim::engine::{Command, Engine};
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: replay <file.state> [ai]");
    let check_ai = std::env::args().nth(2).as_deref() == Some("ai");
    let save = SaveFile::open(&path).unwrap();
    let mut hist: BTreeMap<u16, usize> = BTreeMap::new();
    for c in &save.commands {
        *hist.entry(c.kind).or_default() += 1;
    }
    println!("commands {} kinds {:?}", save.commands.len(), hist);
    if std::env::var("SHOW_GAME_TYPE").is_ok() {
        println!("game_type {} version {}", save.initial.settings.game_type, save.initial.version);
    }
    let mut e = Engine::from_state(save.initial.clone());
    // Only replay the start-match command when the file has one: a weekly challenge scenario that has not been
    // played yet has no commands and its initial state already holds the scenario's techs.
    if save.commands.iter().any(|c| c.kind == 20) {
        e.start_match();
    }
    if let Ok(b) = std::env::var("AI_BONUS") {
        let b: i32 = b.parse().unwrap();
        for p in e.s.players.iter_mut() {
            if p.id != 255 {
                p.currency += b;
            }
        }
    }
    if std::env::var("P1_T0").is_ok() {
        let me = e.current_player();
        e.start_turn(me);
    }
    let limit: usize = std::env::var("LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    let (mut illegal, mut unsupported, mut ai_ok, mut ai_bad) = (0, 0, 0, 0);
    let (mut tile_ok, mut tile_bad) = (0, 0);
    let mut shown = 0;
    let mut topups: BTreeMap<u8, i32> = BTreeMap::new();
    for (i, c) in save.commands.iter().enumerate() {
        if i >= limit {
            break;
        }
        if c.kind == 20 || c.player == 255 {
            continue;
        }
        if c.kind == 22 {
            if e.end_match().is_err() {
                e.s.current_command += 1;
                unsupported += 1;
                if unsupported <= 10 {
                    println!("#{i} unsupported raw command {:?}", c);
                }
            }
            continue;
        }
        if c.kind == 35 {
            if e.resign(c.n as u8).is_err() {
                e.s.current_command += 1;
            }
            continue;
        }
        let Some(cmd) = e.command_from_raw(c) else {
            unsupported += 1;
            if unsupported <= 10 {
                println!("#{i} unsupported raw command {:?}", c);
            }
            // keep the command counter in step
            e.s.current_command += 1;
            continue;
        };
        if e.current_player() != c.player {
            println!(
                "#{i} player mismatch: engine expects {} but command is by {} ({:?})",
                e.current_player(),
                c.player,
                cmd
            );
            break;
        }
        // AI_PLAYERS=2,3: treat these players as AIs for the comparison even when their auto_play flag is off.
        let forced =
            std::env::var("AI_PLAYERS").is_ok_and(|v| v.split(',').any(|x| x.parse::<u8>().ok() == Some(c.player)));
        let auto = forced || e.player(c.player).is_some_and(|p| p.auto_play);
        if check_ai && auto {
            let mut probe = e.clone();
            if let Ok(v) = std::env::var("NOISE_AT") {
                if v.parse::<usize>().ok() == Some(i) {
                    let ns: Vec<String> =
                        (0..16).map(|k| format!("{:.1}", e.stupid_factor(c.player, 101 + k, 0))).collect();
                    println!("NOISE cmd {} p{}: {:?}", e.s.current_command, c.player, ns);
                }
            }
            if std::env::var("AI_RANK").is_ok() {
                probe.ai_rank_report(&cmd);
            }
            let mine = e.ai_get_move();
            let is_tile = matches!(
                cmd,
                Command::Research { .. }
                    | Command::Train { .. }
                    | Command::Build { .. }
                    | Command::EndTurn
                    | Command::CityReward { .. }
            );
            if mine == cmd {
                ai_ok += 1;
                if is_tile {
                    tile_ok += 1;
                }
                if std::env::var("AI_OK").is_ok() {
                    println!("#{i} turn {} AI p{} OK {:?}", e.s.current_turn, c.player, cmd);
                }
            } else {
                ai_bad += 1;
                if is_tile {
                    tile_bad += 1;
                }
                if shown < std::env::var("AI_SHOW").ok().and_then(|v| v.parse().ok()).unwrap_or(25) {
                    shown += 1;
                    println!("#{i} turn {} AI p{} recorded {:?} / mine {:?}", e.s.current_turn, c.player, cmd, mine);
                }
            }
        }
        if let Ok(v) = std::env::var("SETHP") {
            for spec in v.split(';') {
                let f: Vec<i64> = spec.split(':').filter_map(|x| x.parse().ok()).collect();
                if f.len() == 3 && f[0] as usize == i {
                    if let Some(u) = e.s.units.iter_mut().find(|u| u.id as i64 == f[1]) {
                        u.health = f[2] as u16;
                    }
                }
            }
        }
        let free = std::env::var("FREE").is_ok();
        if free {
            let cost = match cmd {
                Command::Train { unit_type, .. } => e.t.unit(unit_type).map(|u| u.data.cost).unwrap_or(0),
                Command::Build { improvement, .. } => {
                    e.t.improvement(improvement).and_then(|i| i.data.cost).unwrap_or(0)
                }
                Command::Research { tech } => e.tech_price(c.player, tech),
                _ => 0,
            };
            let have = e.player(c.player).map(|p| p.currency).unwrap_or(0);
            if have < cost {
                if std::env::var("TOPUP_LOG").is_ok() {
                    println!("TOPUP #{i} turn {} p{} short {} for {:?}", e.s.current_turn, c.player, cost - have, cmd);
                }
                *topups.entry(c.player).or_insert(0) += cost - have;
                e.add_currency(c.player, cost - have);
            }
        }
        let legal = matches!(
            cmd,
            Command::EndTurn
                | Command::Stay { .. }
                | Command::PeaceTreaty { .. }
                | Command::PeaceResponse { .. }
                | Command::BreakPeace { .. }
                | Command::EstablishEmbassy { .. }
        ) || e.legal_commands().contains(&cmd);
        if !legal {
            illegal += 1;
            if illegal <= 25 {
                let extra = match cmd {
                    Command::Train { city, unit_type } => format!(
                        " [stars {} cost {:?} unit_at_city {:?} units_home {} level {:?} unlocked {:?}]",
                        e.player(c.player).map(|p| p.currency).unwrap_or(0),
                        e.t.unit(unit_type).map(|u| u.data.cost),
                        e.unit_at(city).map(|u| (u.id, u.type_idx)),
                        e.s.units.iter().filter(|u| u.home == city && u.owner == c.player).count(),
                        e.improvement_rt(city).map(|r| r.level),
                        e.unlocked_units(c.player, false)
                    ),
                    Command::Move { unit, to } => format!(
                        " [unit {:?} dests {:?} dest_tile {:?} unit_there {:?}]",
                        e.unit(unit).map(|u| (u.coordinates, u.moved, u.attacked, u.type_idx, u.owner, u.health)),
                        e.unit(unit).map(|u| e.move_destinations(u).iter().map(|c| (c.x, c.y)).collect::<Vec<_>>()),
                        e.tile(to).map(|t| (t.terrain, t.improvement, t.owner, t.resource)),
                        e.unit_at(to).map(|u| (u.id, u.owner, u.type_idx))
                    ),
                    Command::CityReward { city, .. } => {
                        format!(" [rt {:?} triggers {:?}]", e.improvement_rt(city), e.triggers)
                    }
                    Command::Research { tech } => format!(
                        " [stars {} price {} researchable {:?} have {:?}]",
                        e.player(c.player).map(|p| p.currency).unwrap_or(0),
                        e.tech_price(c.player, tech),
                        e.researchable(c.player),
                        e.techs(c.player)
                    ),
                    Command::Build { at, improvement } => format!(
                        " [stars {} tile {:?} unlocked {} unit_at {:?} rq {:?}/{:?} trig {:?}]",
                        e.player(c.player).map(|p| p.currency).unwrap_or(0),
                        e.tile(at),
                        e.unlocked_improvements(c.player).contains(&improvement),
                        e.unit_at(at).map(|u| (u.id, u.owner)),
                        e.t.improvement(improvement).map(|i| i.resource_req.clone()),
                        e.t.improvement(improvement).map(|i| i.terrain_req.clone()),
                        e.triggers
                    ),
                    _ => String::new(),
                };
                println!("#{i} turn {} p{} not legal in engine: {:?}{}", e.s.current_turn, c.player, cmd, extra);
            }
        }
        e.apply_unchecked(cmd);
        if let Ok(ct) = std::env::var("CITY_TRACK") {
            let v: Vec<i32> = ct.split(',').filter_map(|x| x.parse().ok()).collect();
            if v.len() == 2 {
                if let Some(rt) = e.improvement_rt(polytopia_sim::WorldCoordinates { x: v[0], y: v[1] }) {
                    println!(
                        "CITY #{i} t{} owner {} lvl {} pop {} xp {} prod {} rw {:?}",
                        e.s.current_turn,
                        e.tile(polytopia_sim::WorldCoordinates { x: v[0], y: v[1] }).map(|t| t.owner).unwrap_or(0),
                        rt.level,
                        rt.population,
                        rt.xp,
                        rt.production,
                        rt.rewards
                    );
                }
            }
        }
        if let Some(pid) = std::env::var("TRACK_STARS").ok().and_then(|v| v.parse::<u8>().ok()) {
            println!(
                "STARS #{i} t{} p{} cmd {:?} -> p{} stars {}",
                e.s.current_turn,
                c.player,
                cmd,
                pid,
                e.player(pid).map(|p| p.currency).unwrap_or(0)
            );
        }
        if let Some(uid) = std::env::var("TRACK_UNIT").ok().and_then(|v| v.parse::<i32>().ok()) {
            if let Some(u) = e.s.units.iter().find(|u| u.id as i32 == uid) {
                println!(
                    "TRACK #{i} t{} u{} hp {} at ({},{}) moved {} att {}",
                    e.s.current_turn, u.id, u.health, u.coordinates.x, u.coordinates.y, u.moved, u.attacked
                );
            }
        }
        if let Ok(v) = std::env::var("TILE_AT") {
            let p: Vec<i32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            if p.len() == 3 && p[2] as usize == i {
                let c = polytopia_sim::WorldCoordinates { x: p[0], y: p[1] };
                println!("TILE at #{i}: {:?} rt {:?}", e.tile(c), e.improvement_rt(c));
                for n in e.neighbors(c) {
                    println!(
                        "  nb {:?}",
                        e.tile(n).map(|t| (
                            t.coordinates.x,
                            t.coordinates.y,
                            t.terrain,
                            t.owner,
                            t.improvement,
                            t.ruling_city
                        ))
                    );
                }
            }
        }
        if std::env::var("DUMP_AT").ok().and_then(|v| v.parse::<usize>().ok()) == Some(i) {
            let mut us: Vec<_> =
                e.s.units
                    .iter()
                    .map(|u| {
                        (
                            u.id,
                            u.owner,
                            u.type_idx,
                            u.coordinates.x,
                            u.coordinates.y,
                            u.direction,
                            u.home.x,
                            u.home.y,
                            u.passenger.as_ref().map(|p| (p.id, p.type_idx, p.health, p.xp, p.promotion)),
                            u.health,
                            (u.xp, u.promotion, u.moved, u.attacked, u.poisoned, e.defence_bonus(u)),
                        )
                    })
                    .collect();
            us.sort();
            for u in us {
                println!("UNIT after #{i}: {:?}", u);
            }
        }
        // OPIN_AT=i:p prints player p's opinion of every other player, by reason, after command i.
        if let Some((at, p)) = std::env::var("OPIN_AT")
            .ok()
            .and_then(|v| v.split_once(':').and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<u8>().ok()?))))
        {
            if at == i {
                for (pid, st) in e.all_military_stats() {
                    println!(
                        "MILSTATS after #{i} p{pid}: {:?} weighted {}",
                        (st.defense, st.attack, st.movement, st.range),
                        st.weighted_total()
                    );
                }
                for o in e.s.players.iter().map(|q| q.id).filter(|&o| o != p && o != 255) {
                    if let Some(st) = e.opinion_reasons(p, o) {
                        println!("OPINION after #{i} p{p} of p{o}: total {} reasons {:?}", st.total, st.reasons);
                    }
                }
            }
        }
        if std::env::var("SLACK").is_ok()
            && matches!(cmd, Command::Train { .. } | Command::Build { .. } | Command::Research { .. })
            && e.player(c.player).is_some_and(|p| p.auto_play)
            && i < 700
        {
            println!("SLACK p{} {}", c.player, e.player(c.player).map(|p| p.currency).unwrap_or(0));
        }
        if let Ok(v) = std::env::var("TRACE_UNTIL") {
            if i < v.parse::<usize>().unwrap_or(0) {
                println!(
                    "  #{i} p{} {:?} -> stars {}",
                    c.player,
                    cmd,
                    e.player(c.player).map(|p| p.currency).unwrap_or(-1)
                );
            }
        }
    }
    println!("topups needed per player: {:?}", topups);
    println!("replayed; turn {} (file says {}), illegal {} unsupported {} ai match {} / mismatch {} | tile-cmd match {} / mismatch {}", e.s.current_turn, save.current.current_turn, illegal, unsupported, ai_ok, ai_bad, tile_ok, tile_bad);
    if std::env::var("UNIT_DIFF").is_ok() {
        let mut bad = 0;
        for fu in &save.current.units {
            match e.s.units.iter().find(|u| u.id == fu.id) {
                None => {
                    println!(
                        "UNITDIFF id {} missing in engine (file: owner {} type {} at ({},{}) hp {})",
                        fu.id, fu.owner, fu.type_idx, fu.coordinates.x, fu.coordinates.y, fu.health
                    );
                    bad += 1;
                }
                Some(u) => {
                    if u.owner != fu.owner
                        || u.type_idx != fu.type_idx
                        || u.coordinates != fu.coordinates
                        || u.health != fu.health
                        || u.xp != fu.xp
                        || u.moved != fu.moved
                        || u.attacked != fu.attacked
                    {
                        println!("UNITDIFF id {} file owner {} type {} at ({},{}) hp {} xp {} mv {}/{} | engine owner {} type {} at ({},{}) hp {} xp {} mv {}/{}", fu.id, fu.owner, fu.type_idx, fu.coordinates.x, fu.coordinates.y, fu.health, fu.xp, fu.moved, fu.attacked, u.owner, u.type_idx, u.coordinates.x, u.coordinates.y, u.health, u.xp, u.moved, u.attacked);
                        bad += 1;
                    }
                }
            }
        }
        for u in &e.s.units {
            if !save.current.units.iter().any(|f| f.id == u.id) {
                println!(
                    "UNITDIFF id {} extra in engine (owner {} type {} at ({},{}) hp {})",
                    u.id, u.owner, u.type_idx, u.coordinates.x, u.coordinates.y, u.health
                );
                bad += 1;
            }
        }
        println!("UNITDIFF total {} of {} units; next id file {:?}", bad, save.current.units.len(), "");
    }
    if std::env::var("SCORE_DIFF").is_ok() {
        // Score inputs of the file's final state against the engine's: owner, improvement, base score, explored.
        for (ft, et) in save.current.map.tiles.iter().zip(e.s.map.tiles.iter()) {
            if ft.owner != et.owner || ft.improvement != et.improvement {
                println!(
                    "SCOREDIFF tile ({},{}) owner file {} eng {} | impr file {} eng {}",
                    ft.coordinates.x, ft.coordinates.y, ft.owner, et.owner, ft.improvement, et.improvement
                );
            }
        }
        for fr in &save.current.improvements {
            if let Some(er) = e.s.improvements.iter().find(|r| r.tile == fr.tile) {
                if er.base_score != fr.base_score || er.owner != fr.owner || er.level != fr.level || er.xp != fr.xp {
                    let t = &save.current.map.tiles[fr.tile as usize];
                    println!("SCOREDIFF impr ({},{}) {} base file {} eng {} | owner file {} eng {} | lvl file {} eng {} | xp file {} eng {}", t.coordinates.x, t.coordinates.y, t.improvement, fr.base_score, er.base_score, fr.owner, er.owner, fr.level, er.level, fr.xp, er.xp);
                }
            }
        }
        let n = save.current.map.tiles.len();
        for i in 0..n {
            for pl in save.current.players.iter().map(|p| p.id).filter(|id| *id != 255) {
                let f = save.current.fog.get(i).is_some_and(|f| f.explorers.contains(&pl));
                let g = e.is_explored(pl, i);
                if f != g {
                    let c = save.current.map.tiles[i].coordinates;
                    println!("SCOREDIFF explored p{} ({},{}) file {} eng {}", pl, c.x, c.y, f, g);
                }
            }
        }
    }
    if std::env::var("PASS_DIFF").is_ok() {
        for fu in &save.current.units {
            if let Some(u) = e.s.units.iter().find(|u| u.id == fu.id) {
                if format!("{:?}", u.passenger) != format!("{:?}", fu.passenger) {
                    println!(
                        "PASSDIFF id {} type {} file passenger {:?} | engine {:?}",
                        fu.id, fu.type_idx, fu.passenger, u.passenger
                    );
                }
            }
        }
    }
    if std::env::var("IMPR_DIFF").is_ok() {
        // Stored improvement runtime (level, xp, production...) of the file's final state against the engine's.
        for fr in &save.current.improvements {
            let t = &save.current.map.tiles[fr.tile as usize];
            let er = e.s.improvements.iter().find(|r| r.tile == fr.tile);
            let et = &e.s.map.tiles[fr.tile as usize];
            let same = er.is_some_and(|r| r.level == fr.level && r.production == fr.production && r.xp == fr.xp)
                && et.improvement == t.improvement;
            if !same || std::env::var("IMPR_ALL").is_ok() {
                println!("IMPRDIFF ({},{}) impr {} owner {} file lvl {} prod {} xp {} | engine impr {} lvl {:?} prod {:?} xp {:?} work {}", t.coordinates.x, t.coordinates.y, t.improvement, t.owner, fr.level, fr.production, fr.xp, et.improvement, er.map(|r| r.level), er.map(|r| r.production), er.map(|r| r.xp), e.tile_work(t.coordinates));
            }
        }
    }
    if std::env::var("EXPLORE_DIFF").is_ok() {
        for pl in save.current.players.iter().map(|p| p.id).filter(|id| *id != 255) {
            let w = save.current.map.width as i32;
            let mut missing = Vec::new();
            let mut extra = Vec::new();
            for (i, t) in save.current.map.tiles.iter().enumerate() {
                let file = save.current.fog.get(i).is_some_and(|f| f.explorers.contains(&pl));
                let eng = e.is_explored(pl, i);
                if file && !eng {
                    missing.push((t.coordinates.x, t.coordinates.y));
                }
                if eng && !file {
                    extra.push((t.coordinates.x, t.coordinates.y));
                }
            }
            let _ = w;
            println!(
                "EXPLDIFF p{} missing {} extra {} | missing {:?} | extra {:?}",
                pl,
                missing.len(),
                extra.len(),
                &missing[..missing.len().min(12)],
                &extra[..extra.len().min(12)]
            );
        }
    }
    if std::env::var("MEET_DIFF").is_ok() {
        for row in &save.current.diplomacy {
            for link in &row.relations {
                let eng =
                    e.s.diplomacy
                        .iter()
                        .find(|r| r.player == row.player)
                        .and_then(|r| r.relations.iter().find(|l| l.other == link.other))
                        .map(|l| l.relation.first_meet);
                if link.relation.first_meet != polytopia_sim::DIPLOMACY_DEFAULT_TURN
                    || eng.map(|m| m != polytopia_sim::DIPLOMACY_DEFAULT_TURN).unwrap_or(false)
                {
                    println!(
                        "MEETDIFF p{} -> p{}: file {} engine {:?}",
                        row.player, link.other, link.relation.first_meet, eng
                    );
                }
            }
        }
    }
    if std::env::var("TASK_DIFF").is_ok() {
        for st in &save.current.tasks {
            let eng = e.s.tasks.iter().find(|t| t.player == st.player);
            println!(
                "TASKS p{} file {:?}",
                st.player,
                st.tasks.iter().map(|x| (x.kind as i32, x.is_completed, x.kills, x.turns)).collect::<Vec<_>>()
            );
            println!(
                "TASKS p{} eng  {:?}",
                st.player,
                eng.map(|r| r
                    .tasks
                    .iter()
                    .map(|x| (x.kind as i32, x.is_completed, x.kills, x.turns))
                    .collect::<Vec<_>>())
            );
        }
    }
    if std::env::var("CITY_DIFF").is_ok() {
        for rt in &save.current.improvements {
            let t = &save.current.map.tiles[rt.tile as usize];
            if t.improvement != 1 {
                continue;
            }
            let eng = e.improvement_rt(t.coordinates);
            println!("CITYDIFF ({},{}) owner file {} eng {:?} | lvl file {} eng {:?} | pop file {} eng {:?} | xp file {} eng {:?}", t.coordinates.x, t.coordinates.y, t.owner,
                e.tile(t.coordinates).map(|x| x.owner), rt.level, eng.map(|r| r.level), rt.population, eng.map(|r| r.population), rt.xp, eng.map(|r| r.xp));
        }
    }
    for p in &save.current.players {
        if p.id == 255 {
            continue;
        }
        let mine = e.player(p.id).copied();
        println!(
            "player {:>3} stars file {:>3} engine {:>3} | score file {:>6} engine {:>6} | cities file {} engine {} | units file {} engine {} | techs file {} engine {}",
            p.id,
            p.currency,
            mine.map(|m| m.currency).unwrap_or(-1),
            p.score,
            e.score(p.id),
            save.current.map.tiles.iter().filter(|t| t.owner == p.id && t.improvement == 1).count(),
            e.city_count(p.id),
            save.current.units.iter().filter(|u| u.owner == p.id).count(),
            e.s.units.iter().filter(|u| u.owner == p.id).count(),
            save.current.tech.iter().find(|t| t.player == p.id).map(|t| t.tech.len()).unwrap_or(0),
            e.techs(p.id).len()
        );
    }
}

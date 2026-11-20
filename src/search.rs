//! Seed search for a Normal Dryland game with 9 players.
//!
//! The capital filter replays the capital placement step and drops a seed
//! before `generate_internal` runs. Hidden tribes are the outer loop. More than
//! three unknown tribes is rejected instead of searched.

use std::ops::Range;

use crate::data::load_embedded_game_logic;
use crate::map::generate::generate_internal;
use crate::map::shape::{create_from_preset, generate_player_capital_positions};
use crate::rng::DotNetRandom;
use crate::{Map, MapPreset, PlayerState, WorldCoordinates, GAME_VERSION};

/// Normal map width. Nine players on this width have one legal tile per cell.
pub const WIDTH: u16 = 16;
pub const PLAYER_COUNT: usize = 9;
pub const WINDOW_SIDE: i32 = 5;
/// Hidden bot tribes. Above this the search returns [`SearchError::TooManyUnknownTribes`].
pub const MAX_HIDDEN_TRIBES: usize = 3;

/// The only legal capital coordinates for Normal width 16 and 9 players.
pub const CAPITAL_CELLS: [(i32, i32); 9] =
    [(3, 3), (8, 3), (13, 3), (3, 8), (8, 8), (13, 8), (3, 13), (8, 13), (13, 13)];

/// One observed tile. `None` in the window is a wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileSample {
    pub terrain: i32,
    pub resource: i32,
    pub climate: i32,
}

/// What the human sees at the start of a Normal Dryland 9-player game.
///
/// `seat` is `0..9`. `None` tries every seat. `tribes` is indexed by seat;
/// `None` is a hidden bot. `window` is the 5×5 centered on `capital`, row-major,
/// north-west corner first. A missing entry is a wildcard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartingView {
    pub seat: Option<usize>,
    pub tribes: Vec<Option<i32>>,
    pub capital: WorldCoordinates,
    pub window: Vec<Option<TileSample>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    TooManyUnknownTribes { hidden: usize },
    InvalidView(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::TooManyUnknownTribes { hidden } => {
                write!(f, "too many unknown tribes ({hidden}); at most {MAX_HIDDEN_TRIBES} can be hidden")
            }
            SearchError::InvalidView(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SearchError {}

/// 5×5 centered on `capital`. Tiles outside the map are `None`.
pub fn window_at(map: &Map, capital: WorldCoordinates) -> Vec<Option<TileSample>> {
    let radius = WINDOW_SIDE / 2;
    let mut window = Vec::with_capacity((WINDOW_SIDE * WINDOW_SIDE) as usize);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            window.push(sample_at(map, capital.x + dx, capital.y + dy));
        }
    }
    window
}

fn sample_at(map: &Map, x: i32, y: i32) -> Option<TileSample> {
    if x < 0 || y < 0 || x >= i32::from(map.width) || y >= i32::from(map.height) {
        return None;
    }
    let index = (y * i32::from(map.width) + x) as usize;
    let tile = map.tiles.get(index)?;
    Some(TileSample { terrain: tile.terrain, resource: tile.resource, climate: tile.climate })
}

/// Seeds in `seeds` whose Dryland map matches `view`.
///
/// `seeds` is half-open. The game draws map seeds from `0..0x7FFF_FFFF`.
/// A hit is a candidate inside this port until a captured retail map matches.
pub fn search(view: &StartingView, seeds: Range<i32>) -> Result<Vec<i32>, SearchError> {
    validate(view)?;
    let hidden = view.tribes.iter().filter(|tribe| tribe.is_none()).count();
    if hidden > MAX_HIDDEN_TRIBES {
        return Err(SearchError::TooManyUnknownTribes { hidden });
    }
    let data = load_embedded_game_logic().expect("built-in rules data");
    let lineups = tribe_lineups(&view.tribes, &tribe_pool(&data, &view.tribes));
    let settings = create_from_preset(MapPreset::Dryland);
    let mut hits = Vec::new();
    let mut seed = seeds.start;
    while seed < seeds.end {
        if capital_matches(seed, view) {
            for lineup in &lineups {
                let mut players = players_for(lineup);
                let mut map = Map::new(WIDTH, WIDTH);
                generate_internal(GAME_VERSION, seed, &mut map, &settings, &mut players, 0, None);
                if window_matches(&map, view) {
                    hits.push(seed);
                    break;
                }
            }
        }
        seed = match seed.checked_add(1) {
            Some(next) => next,
            None => break,
        };
    }
    Ok(hits)
}

fn validate(view: &StartingView) -> Result<(), SearchError> {
    if view.tribes.len() != PLAYER_COUNT {
        return Err(SearchError::InvalidView(format!(
            "expected {PLAYER_COUNT} tribe slots, found {}",
            view.tribes.len()
        )));
    }
    if view.window.len() != (WINDOW_SIDE * WINDOW_SIDE) as usize {
        return Err(SearchError::InvalidView(format!(
            "expected a {}×{} window, found {} tiles",
            WINDOW_SIDE,
            WINDOW_SIDE,
            view.window.len()
        )));
    }
    if !CAPITAL_CELLS.contains(&(view.capital.x, view.capital.y)) {
        return Err(SearchError::InvalidView(format!(
            "capital ({}, {}) is not one of the nine Normal cells",
            view.capital.x, view.capital.y
        )));
    }
    if let Some(seat) = view.seat {
        if seat >= PLAYER_COUNT {
            return Err(SearchError::InvalidView(format!("seat {seat} is outside 0..{PLAYER_COUNT}")));
        }
    }
    Ok(())
}

fn capital_matches(seed: i32, view: &StartingView) -> bool {
    let mut rng = DotNetRandom::new(seed);
    let capitals = generate_player_capital_positions(&mut rng, i32::from(WIDTH), PLAYER_COUNT as i32);
    let want = view.capital.y * i32::from(WIDTH) + view.capital.x;
    match view.seat {
        Some(seat) => capitals.get(seat).copied() == Some(want),
        None => capitals.contains(&want),
    }
}

/// Players as the game builds them for a new match: ids `1..=n` in seat order, then Nature (id 255).
fn players_for(tribes: &[i32]) -> Vec<PlayerState> {
    let mut players: Vec<PlayerState> = tribes
        .iter()
        .enumerate()
        .map(|(seat, tribe)| PlayerState {
            id: seat as u8 + 1,
            handicap: if seat == 0 { 1 } else { 0 },
            currency: 0,
            score: 0,
            tribe: *tribe,
            auto_play: seat != 0,
            start_tile: WorldCoordinates { x: -1, y: -1 },
        })
        .collect();
    players.push(PlayerState {
        id: 255,
        handicap: 1,
        currency: 0,
        score: 0,
        tribe: 1,
        auto_play: true,
        start_tile: WorldCoordinates { x: -1, y: -1 },
    });
    players
}

fn tribe_pool(data: &crate::data::GameLogicData, known: &[Option<i32>]) -> Vec<i32> {
    let mut taken = Vec::new();
    for tribe in known.iter().flatten() {
        if !taken.contains(tribe) {
            taken.push(*tribe);
        }
    }
    let mut pool: Vec<i32> = data.tribes.values().map(|tribe| tribe.idx).filter(|idx| !taken.contains(idx)).collect();
    pool.sort_unstable();
    pool.dedup();
    pool
}

fn tribe_lineups(known: &[Option<i32>], pool: &[i32]) -> Vec<Vec<i32>> {
    let slots: Vec<usize> =
        known.iter().enumerate().filter_map(|(index, tribe)| tribe.is_none().then_some(index)).collect();
    let mut lineups = Vec::new();
    let mut chosen = Vec::new();
    let mut used = vec![false; pool.len()];
    fill_lineups(known, pool, &slots, &mut chosen, &mut used, &mut lineups);
    lineups
}

fn fill_lineups(
    known: &[Option<i32>],
    pool: &[i32],
    slots: &[usize],
    chosen: &mut Vec<i32>,
    used: &mut [bool],
    lineups: &mut Vec<Vec<i32>>,
) {
    if chosen.len() == slots.len() {
        let mut lineup = Vec::with_capacity(known.len());
        for tribe in known {
            lineup.push(tribe.unwrap_or(0));
        }
        for (slot, tribe) in slots.iter().zip(chosen.iter()) {
            lineup[*slot] = *tribe;
        }
        lineups.push(lineup);
        return;
    }
    for (index, tribe) in pool.iter().enumerate() {
        if used[index] {
            continue;
        }
        used[index] = true;
        chosen.push(*tribe);
        fill_lineups(known, pool, slots, chosen, used, lineups);
        chosen.pop();
        used[index] = false;
    }
}

/// Field value meaning "unknown": it matches any terrain, resource or climate.
pub const WILDCARD: i32 = -1;

fn window_matches(map: &Map, view: &StartingView) -> bool {
    let generated = window_at(map, view.capital);
    view.window.iter().zip(generated.iter()).all(|(observed, actual)| match (observed, actual) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(o), Some(a)) => {
            (o.terrain == WILDCARD || o.terrain == a.terrain)
                && (o.resource == WILDCARD || o.resource == a.resource)
                && (o.climate == WILDCARD || o.climate == a.climate)
        }
    })
}

/// Read a [`StartingView`] from the example JSON shape.
///
/// `seat` may be `null`. `tribes` has nine integers or `null`s. `window` has
/// twenty-five tile objects or `null`s; inside a tile object, `terrain`, `resource` and `climate` may each be `null` (any value). Optional `width`, `players`, and
/// `preset` must be 16, 9, and `"dryland"` when present.
pub fn starting_view_from_json(text: &str) -> Result<StartingView, SearchError> {
    let value = parse_json(text)?;
    let obj = value.as_object().ok_or_else(|| SearchError::InvalidView("view must be a JSON object".into()))?;
    if let Some(width) = obj_get(obj, "width") {
        if width.as_i32() != Some(i32::from(WIDTH)) {
            return Err(SearchError::InvalidView("only width 16 is searched".into()));
        }
    }
    if let Some(players) = obj_get(obj, "players") {
        if players.as_i32() != Some(PLAYER_COUNT as i32) {
            return Err(SearchError::InvalidView("only 9 players are searched".into()));
        }
    }
    if let Some(preset) = obj_get(obj, "preset") {
        if preset.as_str() != Some("dryland") {
            return Err(SearchError::InvalidView("only the dryland preset is searched".into()));
        }
    }
    let seat = match obj_get(obj, "seat") {
        None | Some(Json::Null) => None,
        Some(value) => Some(
            usize::try_from(
                value.as_i32().ok_or_else(|| SearchError::InvalidView("seat must be an integer or null".into()))?,
            )
            .map_err(|_| SearchError::InvalidView("seat must be zero or positive".into()))?,
        ),
    };
    let tribes_json = obj_get(obj, "tribes")
        .and_then(Json::as_array)
        .ok_or_else(|| SearchError::InvalidView("tribes must be an array".into()))?;
    let mut tribes = Vec::with_capacity(tribes_json.len());
    for tribe in tribes_json {
        match tribe {
            Json::Null => tribes.push(None),
            Json::Int(idx) => tribes.push(Some(i32_from(*idx)?)),
            _ => return Err(SearchError::InvalidView("each tribe is an integer or null".into())),
        }
    }
    let capital = obj_get(obj, "capital")
        .and_then(Json::as_object)
        .ok_or_else(|| SearchError::InvalidView("capital must be an object".into()))?;
    let x = obj_get(capital, "x")
        .and_then(Json::as_i32)
        .ok_or_else(|| SearchError::InvalidView("capital.x must be an integer".into()))?;
    let y = obj_get(capital, "y")
        .and_then(Json::as_i32)
        .ok_or_else(|| SearchError::InvalidView("capital.y must be an integer".into()))?;
    let window_json = obj_get(obj, "window")
        .and_then(Json::as_array)
        .ok_or_else(|| SearchError::InvalidView("window must be an array".into()))?;
    let mut window = Vec::with_capacity(window_json.len());
    for tile in window_json {
        window.push(parse_tile(tile)?);
    }
    Ok(StartingView { seat, tribes, capital: WorldCoordinates { x, y }, window })
}

fn parse_tile(value: &Json) -> Result<Option<TileSample>, SearchError> {
    match value {
        Json::Null => Ok(None),
        Json::Obj(fields) => {
            let terrain = field_or_wildcard(fields, "terrain")?;
            let resource = field_or_wildcard(fields, "resource")?;
            let climate = field_or_wildcard(fields, "climate")?;
            Ok(Some(TileSample { terrain, resource, climate }))
        }
        _ => Err(SearchError::InvalidView("each window entry is a tile object or null".into())),
    }
}

/// A tile field is an integer, or `null` for "not read from the screenshot" ([`WILDCARD`]).
fn field_or_wildcard(fields: &[(String, Json)], name: &str) -> Result<i32, SearchError> {
    match obj_get(fields, name) {
        Some(Json::Null) => Ok(WILDCARD),
        Some(value) => {
            value.as_i32().ok_or_else(|| SearchError::InvalidView(format!("tile.{name} must be an integer or null")))
        }
        None => Err(SearchError::InvalidView(format!("tile.{name} is missing"))),
    }
}

fn i32_from(value: i64) -> Result<i32, SearchError> {
    i32::try_from(value).map_err(|_| SearchError::InvalidView(format!("integer {value} does not fit i32")))
}

#[derive(Debug)]
enum Json {
    Null,
    Int(i64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Obj(fields) => Some(fields),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items),
            _ => None,
        }
    }

    fn as_i32(&self) -> Option<i32> {
        match self {
            Json::Int(value) => i32::try_from(*value).ok(),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(value) => Some(value),
            _ => None,
        }
    }
}

fn obj_get<'a>(fields: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    fields.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn parse_json(text: &str) -> Result<Json, SearchError> {
    let mut parser = Parser { src: text.as_bytes(), index: 0 };
    let value = parser.parse_value()?;
    parser.skip();
    if parser.index != parser.src.len() {
        return Err(SearchError::InvalidView("trailing data after JSON".into()));
    }
    Ok(value)
}

struct Parser<'a> {
    src: &'a [u8],
    index: usize,
}

impl<'a> Parser<'a> {
    fn skip(&mut self) {
        while self.src.get(self.index).is_some_and(u8::is_ascii_whitespace) {
            self.index += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.src.get(self.index).copied()
    }

    fn eat(&mut self, expected: u8) -> Result<(), SearchError> {
        if self.peek() == Some(expected) {
            self.index += 1;
            Ok(())
        } else {
            Err(SearchError::InvalidView(format!("expected '{}'", expected as char)))
        }
    }

    fn parse_value(&mut self) -> Result<Json, SearchError> {
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", Json::Null),
            Some(b'"') => Ok(Json::Str(self.parse_string()?)),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            _ => Err(SearchError::InvalidView("invalid JSON value".into())),
        }
    }

    fn parse_literal(&mut self, literal: &[u8], value: Json) -> Result<Json, SearchError> {
        self.skip();
        if self.src[self.index..].starts_with(literal) {
            self.index += literal.len();
            Ok(value)
        } else {
            Err(SearchError::InvalidView("invalid JSON literal".into()))
        }
    }

    fn parse_array(&mut self) -> Result<Json, SearchError> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.index += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.parse_value()?);
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b']') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(SearchError::InvalidView("expected ',' or ']'".into())),
            }
        }
        Ok(Json::Arr(items))
    }

    fn parse_object(&mut self) -> Result<Json, SearchError> {
        self.eat(b'{')?;
        let mut fields = Vec::new();
        if self.peek() == Some(b'}') {
            self.index += 1;
            return Ok(Json::Obj(fields));
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(SearchError::InvalidView("object key must be a string".into()));
            }
            let key = self.parse_string()?;
            self.eat(b':')?;
            let value = self.parse_value()?;
            fields.push((key, value));
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b'}') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(SearchError::InvalidView("expected ',' or '}'".into())),
            }
        }
        Ok(Json::Obj(fields))
    }

    fn parse_string(&mut self) -> Result<String, SearchError> {
        self.eat(b'"')?;
        let mut out = String::new();
        while let Some(byte) = self.src.get(self.index).copied() {
            self.index += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escaped = self
                        .src
                        .get(self.index)
                        .copied()
                        .ok_or_else(|| SearchError::InvalidView("truncated string escape".into()))?;
                    self.index += 1;
                    let ch = match escaped {
                        b'"' | b'\\' | b'/' => escaped as char,
                        b'n' => '\n',
                        b't' => '\t',
                        _ => return Err(SearchError::InvalidView("unsupported string escape".into())),
                    };
                    out.push(ch);
                }
                byte if byte < 0x20 => return Err(SearchError::InvalidView("raw control character in string".into())),
                byte => out.push(byte as char),
            }
        }
        Err(SearchError::InvalidView("unterminated string".into()))
    }

    fn parse_number(&mut self) -> Result<Json, SearchError> {
        self.skip();
        let start = self.index;
        if self.src.get(self.index) == Some(&b'-') {
            self.index += 1;
        }
        if self.src.get(self.index).is_none_or(|byte| !byte.is_ascii_digit()) {
            return Err(SearchError::InvalidView("invalid number".into()));
        }
        while self.src.get(self.index).is_some_and(u8::is_ascii_digit) {
            self.index += 1;
        }
        if self.src.get(self.index).is_some_and(|byte| matches!(byte, b'.' | b'e' | b'E')) {
            return Err(SearchError::InvalidView("only integers are accepted in a starting view".into()));
        }
        let text = std::str::from_utf8(&self.src[start..self.index])
            .map_err(|_| SearchError::InvalidView("invalid number".into()))?;
        let value = text.parse::<i64>().map_err(|_| SearchError::InvalidView(format!("invalid integer {text}")))?;
        Ok(Json::Int(value))
    }
}

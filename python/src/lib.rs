//! Python bindings: the `polytopia_sim` module.
//!
//! The surface follows `engine::game` in the Rust crate: games set up by name, commands as text, positions as
//! JSON text, and the built-in AI.

use engine::game::{replay_save, GameMode, NewGame};
use engine::json::Json;
use engine::{BotDifficulty, Engine, MapPreset, MapSize};
use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::sync::{Mutex, MutexGuard};

mod batch;

fn value_error(e: impl ToString) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Turn engine JSON into Python values (dicts, lists, numbers, strings, None).
fn to_py(py: Python<'_>, j: &Json) -> PyResult<PyObject> {
    Ok(match j {
        Json::Null => py.None(),
        Json::Bool(b) => b.into_pyobject(py)?.to_owned().into_any().unbind(),
        Json::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                (*n as i64).into_pyobject(py)?.into_any().unbind()
            } else {
                n.into_pyobject(py)?.into_any().unbind()
            }
        }
        Json::Str(s) => s.into_pyobject(py)?.into_any().unbind(),
        Json::Arr(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(to_py(py, item)?)?;
            }
            list.into_any().unbind()
        }
        Json::Obj(fields) => {
            let dict = PyDict::new(py);
            for (k, v) in fields {
                dict.set_item(k, to_py(py, v)?)?;
            }
            dict.into_any().unbind()
        }
    })
}

/// The game behind a Python `Game`.
#[derive(Clone)]
struct Inner {
    e: Engine,
    /// Commands the AI has played in the current player's turn (the AI ends a turn that runs too long).
    ai_turn_commands: u32,
}

/// A game in progress. The engine keeps caches that are not thread-safe, so it sits behind a lock.
#[pyclass(module = "polytopia_sim")]
struct Game(Mutex<Inner>);

impl Game {
    fn wrap(e: Engine) -> Self {
        Game(Mutex::new(Inner { e, ai_turn_commands: 0 }))
    }

    /// The game, or an error if an earlier call panicked while changing it (it may be half-updated).
    fn lock(&self) -> PyResult<MutexGuard<'_, Inner>> {
        self.0.lock().map_err(|_| {
            PyRuntimeError::new_err("this game was left inconsistent by an earlier error; start a new one")
        })
    }
}

#[pymethods]
impl Game {
    /// A new game. `players` is a list of `(tribe, human)` pairs in turn order, e.g. `[("imperius", True),
    /// ("bardur", False)]`. `map_attempts=1` sets up much faster but gives a different map from the game's.
    #[new]
    #[pyo3(signature = (players, seed=1, size="normal", map="continents", mode="domination", turn_limit=0, score_limit=0, difficulty="normal", version=None, map_attempts=0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        players: Vec<(String, bool)>,
        seed: i32,
        size: &str,
        map: &str,
        mode: &str,
        turn_limit: i32,
        score_limit: i32,
        difficulty: &str,
        version: Option<i32>,
        map_attempts: u32,
    ) -> PyResult<Self> {
        let mut g = NewGame::new()
            .seed(seed)
            .size(size.parse::<MapSize>().map_err(value_error)?)
            .map_type(map.parse::<MapPreset>().map_err(value_error)?)
            .mode(mode.parse::<GameMode>().map_err(value_error)?)
            .turn_limit(turn_limit)
            .score_limit(score_limit)
            .difficulty(difficulty.parse::<BotDifficulty>().map_err(value_error)?)
            .map_attempts(map_attempts);
        if let Some(v) = version {
            g = g.version(v);
        }
        for (tribe, human) in &players {
            g = if *human { g.human(tribe) } else { g.ai(tribe) };
        }
        Ok(Game::wrap(g.build().map_err(value_error)?))
    }

    /// Open a game file: a position (`.json`), a save from the game (`.state`), or `initial.bin+current.bin`.
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        Ok(Game::wrap(Engine::open_save(path).map_err(PyIOError::new_err)?))
    }

    /// A game from position JSON text.
    #[staticmethod]
    fn from_position(text: &str) -> PyResult<Self> {
        let json = Json::parse(text).map_err(value_error)?;
        Ok(Game::wrap(Engine::from_position(&json).map_err(value_error)?))
    }

    /// The position as JSON text.
    fn to_position(&self) -> PyResult<String> {
        Ok(self.lock()?.e.to_position().to_string())
    }

    /// The current player's legal commands, as text.
    fn legal_moves(&self) -> PyResult<Vec<String>> {
        Ok(self.lock()?.e.legal_commands().iter().map(|c| c.to_string()).collect())
    }

    /// The current player's legal commands as dicts (`text`, `kind`, `unit`, `from`, `to`, ...).
    fn legal_moves_info(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let g = self.lock()?;
        g.e.legal_commands().iter().map(|c| to_py(py, &g.e.command_json(c))).collect()
    }

    /// Play a legal command given as text. Raises `ValueError` if it is not legal now.
    fn play(&self, command: &str) -> PyResult<()> {
        let mut g = self.lock()?;
        let c = g.e.parse_command(command).map_err(value_error)?;
        g.e.apply(c).map_err(|r| value_error(format!("{command}: refused ({r:?})")))?;
        g.ai_turn_commands = 0;
        Ok(())
    }

    /// The command the built-in AI would play now, without playing it.
    fn ai_move(&self) -> PyResult<String> {
        Ok(self.lock()?.e.clone().ai_get_move().to_string())
    }

    /// Let the built-in AI play one command for the current player; returns it, or None when the game is over.
    fn step_ai(&self) -> PyResult<Option<String>> {
        let g = &mut *self.lock()?;
        Ok(g.e.step_ai_cmd(&mut g.ai_turn_commands).map(|c| c.to_string()))
    }

    /// Let the built-in AI play until the game ends or `max_turns` is reached; returns the number of commands.
    #[pyo3(signature = (max_turns=200))]
    fn play_out(&self, max_turns: u32) -> PyResult<u64> {
        let g = &mut *self.lock()?;
        let mut n = 0;
        while !g.e.is_over() && g.e.s.current_turn < max_turns && g.e.step_ai_cmd(&mut g.ai_turn_commands).is_some() {
            n += 1;
        }
        Ok(n)
    }

    /// An independent copy, for search.
    fn copy(&self) -> PyResult<Self> {
        Ok(Game(Mutex::new(self.lock()?.clone())))
    }

    fn __copy__(&self) -> PyResult<Self> {
        self.copy()
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.copy()
    }

    #[getter]
    fn turn(&self) -> PyResult<u32> {
        Ok(self.lock()?.e.s.current_turn)
    }

    #[getter]
    fn current_player(&self) -> PyResult<u8> {
        Ok(self.lock()?.e.current_player())
    }

    #[getter]
    fn is_over(&self) -> PyResult<bool> {
        Ok(self.lock()?.e.is_over())
    }

    #[getter]
    fn game_version(&self) -> PyResult<i32> {
        Ok(self.lock()?.e.s.version)
    }

    /// `(player, score)` for every seat.
    fn scores(&self) -> PyResult<Vec<(u8, i32)>> {
        Ok(self.lock()?.e.scores())
    }

    /// The winners once the game is over, else an empty list.
    fn winners(&self) -> PyResult<Vec<u8>> {
        Ok(self.lock()?.e.game_winners())
    }

    /// `(player, tribe name)` for every seat.
    fn tribes(&self) -> PyResult<Vec<(u8, String)>> {
        let g = self.lock()?;
        let v = g.e.s.version;
        Ok(g.e
            .scores()
            .iter()
            .map(|&(id, _)| (id, g.e.player(id).and_then(|p| engine::game::tribe_name(v, p.tribe)).unwrap_or_default()))
            .collect())
    }

    fn __repr__(&self) -> PyResult<String> {
        let g = self.lock()?;
        Ok(format!(
            "<Game version {} turn {} player {} to move{}>",
            g.e.s.version,
            g.e.s.current_turn,
            g.e.current_player(),
            if g.e.is_over() { ", over" } else { "" }
        ))
    }
}

/// Replay a save from the game and compare the end with the file. Returns a dict: `commands`, `illegal` (list of
/// `(record, command text)`), `skipped`, `ai_agreed`, `ai_differed`, `differences` (list of text) and `game`.
#[pyfunction]
#[pyo3(signature = (path, check_ai=false))]
fn replay(py: Python<'_>, path: &str, check_ai: bool) -> PyResult<PyObject> {
    let r = replay_save(path, check_ai).map_err(PyIOError::new_err)?;
    let d = PyDict::new(py);
    d.set_item("commands", r.commands)?;
    d.set_item("illegal", r.illegal.iter().map(|(i, c)| (*i, c.to_string())).collect::<Vec<_>>())?;
    d.set_item("skipped", r.skipped)?;
    d.set_item("ai_agreed", r.ai_agreed)?;
    d.set_item("ai_differed", r.ai_differed)?;
    d.set_item("differences", r.differences)?;
    d.set_item("game", Game::wrap(r.engine))?;
    Ok(d.into_any().unbind())
}

#[pymodule]
fn polytopia_sim(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Game>()?;
    m.add_function(wrap_pyfunction!(replay, m)?)?;
    m.add("GAME_VERSION", engine::GAME_VERSION)?;
    m.add("MIN_NEW_GAME_VERSION", engine::MIN_NEW_GAME_VERSION)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    batch::register(m)?;
    Ok(())
}

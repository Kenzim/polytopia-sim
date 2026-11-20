//! Map generation pipeline.
//!
//! `generate` is the game's map generator entry point. `generate_internal` is
//! its internal generator. Both call the shape, terrain,
//! and contents functions that are already ported. Steps with no port are skipped
//! in place so the remaining calls stay in order.

pub mod contents;
pub mod generate;
pub mod shape;
pub mod terrain;

/// A set of indices, for the generator's visited and exhausted sets: values in `0..len` are flags, any other value
/// goes to a list, so it behaves like a hash set of the same values.
#[derive(Debug, Clone, Default)]
pub struct IndexSet {
    flags: Vec<bool>,
    other: Vec<i64>,
}

impl IndexSet {
    pub fn with_len(len: usize) -> Self {
        Self { flags: vec![false; len], other: Vec::new() }
    }

    /// Adds the value; true when it was not in the set.
    pub fn insert(&mut self, value: i64) -> bool {
        match usize::try_from(value).ok().and_then(|i| self.flags.get_mut(i)) {
            Some(f) => !std::mem::replace(f, true),
            None if self.other.contains(&value) => false,
            None => {
                self.other.push(value);
                true
            }
        }
    }

    pub fn contains(&self, value: i64) -> bool {
        match usize::try_from(value).ok().and_then(|i| self.flags.get(i)) {
            Some(&f) => f,
            None => self.other.contains(&value),
        }
    }
}

/// Each tile's neighbours at distance 1 (diagonals included, centre excluded) that are on the map, in the order of
/// the generator's area iterators: x offset outer, y offset inner, both from -1.
pub(crate) struct Ring1 {
    width: i32,
    height: i32,
    len: usize,
    /// Eight slots per tile index; the first `count[i]` are used.
    around: Box<[u32]>,
    count: Box<[u8]>,
}

impl Ring1 {
    fn new(width: i32, height: i32, len: usize) -> Self {
        let cells = usize::try_from(width.max(0)).unwrap_or(0) * usize::try_from(height.max(0)).unwrap_or(0);
        let mut around = vec![0u32; cells * 8].into_boxed_slice();
        let mut count = vec![0u8; cells].into_boxed_slice();
        for y in 0..height {
            for x in 0..width {
                let k = (y * width + x) as usize;
                let mut n = 0;
                for (dx, dy) in crate::rules::path::square_offsets(1) {
                    let (nx, ny) = (x + dx, y + dy);
                    if (dx, dy) == (0, 0) || nx < 0 || ny < 0 || nx >= width || ny >= height {
                        continue;
                    }
                    let i = (ny * width + nx) as usize;
                    if i < len {
                        around[k * 8 + n] = i as u32;
                        n += 1;
                    }
                }
                count[k] = n as u8;
            }
        }
        Ring1 { width, height, len, around, count }
    }

    /// The table for the map's size, shared per thread.
    pub(crate) fn for_map(map: &crate::types::Map) -> std::rc::Rc<Ring1> {
        let (width, height, len) = (i32::from(map.width), i32::from(map.height), map.tiles.len());
        RING1.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(r) = cache.iter().find(|r| (r.width, r.height, r.len) == (width, height, len)) {
                return r.clone();
            }
            if cache.len() >= 8 {
                cache.clear();
            }
            let r = std::rc::Rc::new(Ring1::new(width, height, len));
            cache.push(r.clone());
            r
        })
    }

    /// The neighbours of the tile at `(x, y)`, or `None` when that is off the map.
    #[inline]
    pub(crate) fn around(&self, x: i32, y: i32) -> Option<&[u32]> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        let k = (y * self.width + x) as usize;
        Some(&self.around[k * 8..k * 8 + usize::from(self.count[k])])
    }
}

thread_local! {
    static RING1: std::cell::RefCell<Vec<std::rc::Rc<Ring1>>> = const { std::cell::RefCell::new(Vec::new()) };
}

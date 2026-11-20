//! A small JSON reader and writer (the crate has no dependencies), enough for position files.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    /// Keys keep their file order.
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            Json::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().map(|n| n as i64)
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            Json::Num(n) => Some(*n != 0.0),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> &[Json] {
        match self {
            Json::Arr(a) => a,
            _ => &[],
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
    /// `obj[key]` as an integer, or `default` when missing or null.
    pub fn int(&self, key: &str, default: i64) -> i64 {
        self.get(key).and_then(Json::as_i64).unwrap_or(default)
    }
    pub fn flag(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Json::as_bool).unwrap_or(default)
    }
    /// `[x, y]` pair under `key`.
    pub fn pair(&self, key: &str) -> Option<(i32, i32)> {
        let a = self.get(key)?.as_arr();
        Some((a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32))
    }

    pub fn parse(text: &str) -> Result<Json, String> {
        let mut p = Parser { b: text.as_bytes(), i: 0 };
        let v = p.value()?;
        p.ws();
        if p.i != p.b.len() {
            return Err(format!("trailing data at byte {}", p.i));
        }
        Ok(v)
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    let _ = write!(out, "{}", *n as i64);
                } else {
                    let _ = write!(out, "{n}");
                }
            }
            Json::Str(s) => quote(s, out),
            Json::Arr(a) => {
                out.push('[');
                for (k, v) in a.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Json::Obj(kv) => {
                out.push('{');
                for (k, (key, v)) in kv.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    quote(key, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        self.write(&mut s);
        f.write_str(&s)
    }
}

impl From<bool> for Json {
    fn from(b: bool) -> Self {
        Json::Bool(b)
    }
}
macro_rules! num_from {
    ($($t:ty),*) => {$(impl From<$t> for Json { fn from(n: $t) -> Self { Json::Num(n as f64) } })*};
}
num_from!(i8, u8, i16, u16, i32, u32, i64, u64, usize, f32, f64);
impl From<&str> for Json {
    fn from(s: &str) -> Self {
        Json::Str(s.to_string())
    }
}
impl From<String> for Json {
    fn from(s: String) -> Self {
        Json::Str(s)
    }
}
impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(v: Vec<T>) -> Self {
        Json::Arr(v.into_iter().map(Into::into).collect())
    }
}

/// Object builder: `obj().set("a", 1).set("b", true).build()`.
#[derive(Default)]
pub struct ObjBuilder(Vec<(String, Json)>);

pub fn obj() -> ObjBuilder {
    ObjBuilder::default()
}

impl ObjBuilder {
    pub fn set(mut self, key: &str, v: impl Into<Json>) -> Self {
        self.0.push((key.to_string(), v.into()));
        self
    }
    /// Set only when `cond` holds (keeps files short: defaults are left out).
    pub fn set_if(self, cond: bool, key: &str, v: impl Into<Json>) -> Self {
        if cond {
            self.set(key, v)
        } else {
            self
        }
    }
    pub fn build(self) -> Json {
        Json::Obj(self.0)
    }
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("JSON: {what} at byte {}", self.i))
    }
    fn lit(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            self.err("bad literal")
        }
    }
    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.b.get(self.i) {
            None => self.err("unexpected end"),
            Some(b'n') => self.lit("null", Json::Null),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Json::Arr(a));
                }
                loop {
                    a.push(self.value()?);
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Json::Arr(a));
                        }
                        _ => return self.err("expected , or ]"),
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut kv = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Json::Obj(kv));
                }
                loop {
                    self.ws();
                    if self.b.get(self.i) != Some(&b'"') {
                        return self.err("expected key");
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.b.get(self.i) != Some(&b':') {
                        return self.err("expected :");
                    }
                    self.i += 1;
                    kv.push((k, self.value()?));
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Json::Obj(kv));
                        }
                        _ => return self.err("expected , or }"),
                    }
                }
            }
            Some(_) => {
                let st = self.i;
                while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    self.i += 1;
                }
                let s = std::str::from_utf8(&self.b[st..self.i]).unwrap_or("");
                s.parse::<f64>().map(Json::Num).or_else(|_| self.err("bad number"))
            }
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            match self.b.get(self.i) {
                None => return self.err("unterminated string"),
                Some(b'"') => {
                    self.i += 1;
                    return String::from_utf8(out).or_else(|_| self.err("bad utf-8"));
                }
                Some(b'\\') => {
                    let e = *self.b.get(self.i + 1).ok_or("JSON: bad escape")?;
                    self.i += 2;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = std::str::from_utf8(self.b.get(self.i..self.i + 4).ok_or("JSON: bad \\u")?)
                                .unwrap_or("");
                            let mut cp = u32::from_str_radix(hex, 16).map_err(|_| "JSON: bad \\u".to_string())?;
                            self.i += 4;
                            if (0xD800..0xDC00).contains(&cp) && self.b.get(self.i..self.i + 2) == Some(b"\\u") {
                                let hex2 = std::str::from_utf8(&self.b[self.i + 2..self.i + 6]).unwrap_or("");
                                if let Ok(lo) = u32::from_str_radix(hex2, 16) {
                                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                    self.i += 6;
                                }
                            }
                            let c = char::from_u32(cp).unwrap_or('\u{fffd}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                        }
                        c => out.push(c),
                    }
                }
                Some(&c) => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let text = r#"{"a":[1,-2.5,true,null],"b":{"c":"x\"yé"},"d":[]}"#;
        let v = Json::parse(text).unwrap();
        assert_eq!(v.get("a").unwrap().as_arr()[1].as_f64(), Some(-2.5));
        assert_eq!(v.get("b").unwrap().get("c").unwrap().as_str(), Some("x\"yé"));
        assert_eq!(Json::parse(&v.to_string()).unwrap(), v);
    }
}

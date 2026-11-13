//! A tiny JSON reader for the small request bodies this server accepts.
//!
//! The two payloads are one key (`{"code":12}`) and a short macro
//! (`{"steps":[...]}`), so a full JSON library would be dead weight in a
//! statically linked ARM binary.  This parses RFC 8259 values and stops there.

/// A parsed JSON value.  Objects keep insertion order; lookups are linear.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// Field lookup on an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(fields) => fields
                .iter()
                .find(|(k, _)| k.as_str() == key)
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// Non-negative integer field, if the number is exactly representable.
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::Num(n) if n.fract() == 0.0 && *n >= 0.0 && *n <= u32::MAX as f64 => {
                Some(*n as u32)
            }
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(items) => Some(items),
            _ => None,
        }
    }
}

/// Parse one complete JSON document.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        buf: text.as_bytes(),
        pos: 0,
    };
    let value = p.value()?;
    p.skip_ws();
    if p.pos != p.buf.len() {
        return Err(format!("trailing characters at byte {}", p.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while matches!(self.buf.get(self.pos), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.buf.get(self.pos) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(c) if *c == b'-' || c.is_ascii_digit() => self.number(),
            _ => Err(format!("expected a value at byte {}", self.pos)),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, String> {
        let end = self.pos + word.len();
        if self.buf.get(self.pos..end) == Some(word.as_bytes()) {
            self.pos = end;
            Ok(value)
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        while matches!(
            self.buf.get(self.pos),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
        ) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.buf[start..self.pos]).map_err(|_| "bad number")?;
        text.parse::<f64>()
            .map(Value::Num)
            .map_err(|_| format!("invalid number at byte {start}"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            let c = *self
                .buf
                .get(self.pos)
                .ok_or_else(|| "unterminated string".to_string())?;
            self.pos += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = *self
                        .buf
                        .get(self.pos)
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self
                                .buf
                                .get(self.pos..self.pos + 4)
                                .ok_or_else(|| "truncated \\u escape".to_string())?;
                            self.pos += 4;
                            let code = u16::from_str_radix(
                                std::str::from_utf8(hex).map_err(|_| "bad \\u escape")?,
                                16,
                            )
                            .map_err(|_| "bad \\u escape")?;
                            out.push(char::from_u32(code as u32).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(format!("unknown escape at byte {}", self.pos - 1)),
                    }
                }
                _ => out.push(c as char),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.pos += 1; // [
        let mut items = Vec::new();
        self.skip_ws();
        if self.buf.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.buf.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
            }
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.pos += 1; // {
        let mut fields = Vec::new();
        self.skip_ws();
        if self.buf.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Value::Obj(fields));
        }
        loop {
            self.skip_ws();
            if self.buf.get(self.pos) != Some(&b'"') {
                return Err(format!("expected a field name at byte {}", self.pos));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.buf.get(self.pos) != Some(&b':') {
                return Err(format!("expected ':' at byte {}", self.pos));
            }
            self.pos += 1;
            fields.push((key, self.value()?));
            self.skip_ws();
            match self.buf.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Obj(fields));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_request() {
        let v = parse(r#"{"code":12,"name":"RC_CH_UP","type":0,"release":true}"#).unwrap();
        assert_eq!(v.get("code").and_then(Value::as_u32), Some(12));
        assert_eq!(v.get("release").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn parses_macro_steps() {
        let v = parse(r#"{"steps":[{"code":12,"delay_ms":300},{"name":"RC_BACK"}]}"#).unwrap();
        let steps = v.get("steps").and_then(Value::as_array).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[1].get("name").and_then(Value::as_str),
            Some("RC_BACK")
        );
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(parse("{}x").is_err());
        assert!(parse("").is_err());
    }
}

//! Minimal parser for Valve's KeyValues (VDF / ACF) text format.
//!
//! The format is just nested `"key" "value"` pairs or `"key" { ... }`
//! blocks. Comments (`//`) and whitespace are ignored. We only need the
//! fields `appid` and `name` out of `appmanifest_*.acf`, plus the
//! `path` entries inside `libraryfolders.vdf`, so a purpose-built
//! recursive-descent parser is enough — a crate dependency is overkill.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vdf {
    String(String),
    Object(HashMap<String, Vdf>),
}

impl Vdf {
    pub fn as_object(&self) -> Option<&HashMap<String, Vdf>> {
        match self {
            Vdf::Object(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Vdf> {
        self.as_object().and_then(|m| m.get(key))
    }
}

pub fn parse(input: &str) -> Result<Vdf, String> {
    let mut parser = Parser {
        chars: input.chars().collect(),
        pos: 0,
    };
    let mut root = HashMap::new();
    while parser.skip_whitespace_and_comments() {
        let key = parser.read_string()?;
        parser.skip_whitespace_and_comments();
        let value = parser.read_value()?;
        root.insert(key, value);
    }
    Ok(Vdf::Object(root))
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    /// Returns true if more non-whitespace content remains.
    fn skip_whitespace_and_comments(&mut self) -> bool {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                }
                Some('/') if self.chars.get(self.pos + 1) == Some(&'/') => {
                    while let Some(c) = self.advance() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
        self.peek().is_some()
    }

    fn read_string(&mut self) -> Result<String, String> {
        match self.advance() {
            Some('"') => {}
            Some(c) => return Err(format!("expected '\"', got '{}'", c)),
            None => return Err("unexpected end of input".into()),
        }
        let mut buf = String::new();
        while let Some(c) = self.advance() {
            match c {
                '"' => return Ok(buf),
                '\\' => {
                    if let Some(next) = self.advance() {
                        buf.push(match next {
                            'n' => '\n',
                            't' => '\t',
                            '\\' => '\\',
                            '"' => '"',
                            other => other,
                        });
                    }
                }
                other => buf.push(other),
            }
        }
        Err("unterminated string".into())
    }

    fn read_value(&mut self) -> Result<Vdf, String> {
        self.skip_whitespace_and_comments();
        match self.peek() {
            Some('"') => self.read_string().map(Vdf::String),
            Some('{') => {
                self.advance();
                let mut map = HashMap::new();
                loop {
                    if !self.skip_whitespace_and_comments() {
                        return Err("unterminated object".into());
                    }
                    if self.peek() == Some('}') {
                        self.advance();
                        return Ok(Vdf::Object(map));
                    }
                    let key = self.read_string()?;
                    self.skip_whitespace_and_comments();
                    let value = self.read_value()?;
                    map.insert(key, value);
                }
            }
            Some(c) => Err(format!("unexpected character '{}'", c)),
            None => Err("unexpected end of input".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_pair() {
        let v = parse(r#""name" "CS2""#).unwrap();
        assert_eq!(v.get("name").and_then(Vdf::as_str), Some("CS2"));
    }

    #[test]
    fn parses_nested_object() {
        let v = parse(
            r#""AppState"
            {
                "appid"   "730"
                "name"    "Counter-Strike 2"
            }"#,
        )
        .unwrap();
        let app = v.get("AppState").unwrap();
        assert_eq!(app.get("appid").and_then(Vdf::as_str), Some("730"));
        assert_eq!(
            app.get("name").and_then(Vdf::as_str),
            Some("Counter-Strike 2")
        );
    }

    #[test]
    fn ignores_comments() {
        let v = parse(
            r#"
            // top level comment
            "k" "v" // trailing
            "#,
        )
        .unwrap();
        assert_eq!(v.get("k").and_then(Vdf::as_str), Some("v"));
    }
}

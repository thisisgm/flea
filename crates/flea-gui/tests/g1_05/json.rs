//! A JSON value just large enough for the oracle vectors, so the test needs no dependency.
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<V>),
    Map(Vec<(String, V)>),
}

pub fn map(pairs: Vec<(&str, V)>) -> V {
    V::Map(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

impl From<bool> for V {
    fn from(value: bool) -> Self {
        V::Bool(value)
    }
}

impl From<f64> for V {
    fn from(value: f64) -> Self {
        assert!(value.is_finite(), "a vector holds only finite numbers");
        V::Num(value)
    }
}

impl From<usize> for V {
    fn from(value: usize) -> Self {
        V::Num(value as f64)
    }
}

impl From<i64> for V {
    fn from(value: i64) -> Self {
        V::Num(value as f64)
    }
}

impl From<u64> for V {
    fn from(value: u64) -> Self {
        V::Num(value as f64)
    }
}

impl From<&str> for V {
    fn from(value: &str) -> Self {
        V::Str(value.to_owned())
    }
}

impl From<String> for V {
    fn from(value: String) -> Self {
        V::Str(value)
    }
}

impl<T: Into<V>> From<Option<T>> for V {
    fn from(value: Option<T>) -> Self {
        value.map_or(V::Null, Into::into)
    }
}

impl<T: Into<V>> From<Vec<T>> for V {
    fn from(value: Vec<T>) -> Self {
        V::List(value.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<V> + Clone> From<&[T]> for V {
    fn from(value: &[T]) -> Self {
        V::List(value.iter().cloned().map(Into::into).collect())
    }
}

/// A string with every unit past ASCII spelled as an escape, so the runner reads it whatever encoding it assumes.
fn quoted(out: &mut fmt::Formatter<'_>, text: &str) -> fmt::Result {
    out.write_str("\"")?;
    for unit in text.encode_utf16() {
        match unit {
            0x22 => out.write_str("\\\"")?,
            0x5c => out.write_str("\\\\")?,
            0x20..=0x7e => write!(out, "{}", char::from(unit as u8))?,
            _ => write!(out, "\\u{unit:04x}")?,
        }
    }
    out.write_str("\"")
}

impl fmt::Display for V {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            V::Null => out.write_str("null"),
            V::Bool(value) => write!(out, "{value}"),
            V::Num(value) => write!(out, "{value}"),
            V::Str(value) => quoted(out, value),
            V::List(items) => {
                out.write_str("[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.write_str(",")?;
                    }
                    write!(out, "{item}")?;
                }
                out.write_str("]")
            }
            V::Map(pairs) => {
                out.write_str("{")?;
                for (index, (key, item)) in pairs.iter().enumerate() {
                    if index > 0 {
                        out.write_str(",")?;
                    }
                    quoted(out, key)?;
                    write!(out, ":{item}")?;
                }
                out.write_str("}")
            }
        }
    }
}

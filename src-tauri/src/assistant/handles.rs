//! Handles stand in for secret values wherever the model reads Coffer: `[n12.0]` is secret 0 of
//! note 12, `[p3.0]` one in project 3's overview, `[t4.0]` one in todo 4's description, and
//! `[n12]` the whole body of secure note 12. A handle names where a value lives, so tools build
//! them from templates without decrypting anything, and only the user resolves them.
//! Mirrored by `src/lib/handles.ts`.

use std::fmt;

use crate::{
    error::{AppError, AppResult},
    secrets,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Note,
    Overview,
    Todo,
}

impl Source {
    fn letter(self) -> char {
        match self {
            Self::Note => 'n',
            Self::Overview => 'p',
            Self::Todo => 't',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub source: Source,
    pub id: i64,
    /// The secret's position in its template; `None` for a whole secure note.
    pub index: Option<usize>,
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}{}", self.source.letter(), self.id)?;
        if let Some(index) = self.index {
            write!(f, ".{index}")?;
        }
        write!(f, "]")
    }
}

impl Handle {
    /// Parses `n12.0` or `[n12.0]`.
    pub fn parse(text: &str) -> AppResult<Self> {
        let invalid = || AppError::Invalid(format!("Invalid secret handle: {text}"));
        let inner = text.trim();
        let inner = inner
            .strip_prefix('[')
            .and_then(|t| t.strip_suffix(']'))
            .unwrap_or(inner);
        let mut chars = inner.chars();
        let source = match chars.next() {
            Some('n') => Source::Note,
            Some('p') => Source::Overview,
            Some('t') => Source::Todo,
            _ => return Err(invalid()),
        };
        let rest = chars.as_str();
        let (id, index) = match rest.split_once('.') {
            Some((id, index)) => (id, Some(index.parse().map_err(|_| invalid())?)),
            None => (rest, None),
        };
        let id = id.parse::<u32>().map_err(|_| invalid())?.into();
        if index.is_none() && source != Source::Note {
            return Err(invalid());
        }
        Ok(Self { source, id, index })
    }
}

/// A readable template with every `` `!n!` `` marker replaced by its handle.
pub fn mask(template: &str, source: Source, id: i64) -> String {
    secrets::replace_markers(template, |index| {
        Handle {
            source,
            id,
            index: Some(index),
        }
        .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_markers_and_keeps_copyable_values() {
        assert_eq!(
            mask("user: `apo`\npass: `!0!` key `!1!`", Source::Note, 12),
            "user: `apo`\npass: [n12.0] key [n12.1]"
        );
        assert_eq!(mask("no secrets", Source::Todo, 1), "no secrets");
    }

    #[test]
    fn parses_what_it_prints() {
        for text in ["[n12.0]", "[p3.1]", "[t4.0]", "[n7]"] {
            assert_eq!(Handle::parse(text).unwrap().to_string(), text);
        }
        assert_eq!(Handle::parse(" n12.3 ").unwrap().index, Some(3));
        for bad in ["[x1.0]", "[n]", "[n1.]", "[p3]", "[n-1.0]", "n1.0.0", ""] {
            assert!(Handle::parse(bad).is_err(), "{bad}");
        }
    }
}

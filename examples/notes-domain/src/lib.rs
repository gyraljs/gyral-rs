//! Pure, validated values for the synthetic notes example.
#![forbid(unsafe_code)]

/// A rejected field at an input or persistence boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// A note identifier must be a positive integer.
    #[error("invalid note identifier")]
    Id,
    /// Notebook capabilities are 32 bytes encoded as lowercase hexadecimal.
    #[error("invalid notebook capability")]
    Notebook,
    /// Titles contain 1–100 Unicode scalar values without control characters.
    #[error("Use 1–100 characters without control characters for the title.")]
    Title,
    /// Bodies contain at most 4,000 scalar values; tabs and newlines are allowed.
    #[error(
        "Use at most 4,000 characters for the note. Only tabs and newlines may be control characters."
    )]
    Body,
}

/// A positive, database-assigned note identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteId(i64);
impl TryFrom<i64> for NoteId {
    type Error = ParseError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(ParseError::Id)
        }
    }
}
impl TryFrom<String> for NoteId {
    type Error = ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ParseError::Id);
        }
        Self::try_from(value.parse::<i64>().map_err(|_| ParseError::Id)?)
    }
}
impl NoteId {
    /// The validated value for serialization or a query parameter.
    pub const fn value(self) -> i64 {
        self.0
    }
}

/// An anonymous notebook capability. Never log or expose it in a URL.
#[derive(Clone, PartialEq, Eq)]
pub struct NotebookKey(String);
impl std::fmt::Debug for NotebookKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("NotebookKey([REDACTED])")
    }
}
impl TryFrom<String> for NotebookKey {
    type Error = ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(value))
        } else {
            Err(ParseError::Notebook)
        }
    }
}
impl NotebookKey {
    /// Borrow the capability only at its cookie or storage boundary.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// A trimmed, nonempty title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Title(String);
impl TryFrom<String> for Title {
    type Error = ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if (1..=100).contains(&trimmed.chars().count()) && !trimmed.chars().any(char::is_control) {
            Ok(Self(trimmed.to_owned()))
        } else {
            Err(ParseError::Title)
        }
    }
}
impl Title {
    /// The validated title.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Plain text; HTML and Markdown are never interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteBody(String);
impl TryFrom<String> for NoteBody {
    type Error = ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        // Native forms encode newlines as CRLF. Canonicalize that at the boundary.
        let normalized = value.replace("\r\n", "\n");
        if normalized.chars().count() <= 4000
            && !normalized
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            Ok(Self(normalized))
        } else {
            Err(ParseError::Body)
        }
    }
}
impl NoteBody {
    /// The validated body.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// All field failures from one submission, for accessible form feedback.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid note fields")]
pub struct FieldErrors {
    /// Title failure, if any.
    pub title: Option<ParseError>,
    /// Body failure, if any.
    pub body: Option<ParseError>,
}

/// Fully decoded note content; invalid content cannot reach a write port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteContent {
    /// Validated title.
    pub title: Title,
    /// Validated body.
    pub body: NoteBody,
}
impl TryFrom<(String, String)> for NoteContent {
    type Error = FieldErrors;
    fn try_from((title, body): (String, String)) -> Result<Self, Self::Error> {
        let title = Title::try_from(title);
        let body = NoteBody::try_from(body);
        match (title, body) {
            (Ok(title), Ok(body)) => Ok(Self { title, body }),
            (title, body) => Err(FieldErrors {
                title: title.err(),
                body: body.err(),
            }),
        }
    }
}

/// A validated record. Ownership is enforced by the store, not sent to views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Database identifier.
    pub id: NoteId,
    /// Validated content.
    pub content: NoteContent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn boundaries_and_accumulated_errors() {
        assert!(NoteId::try_from(0).is_err());
        for raw in ["-1", "+1", "1.0", "9223372036854775808"] {
            assert!(NoteId::try_from(raw.to_owned()).is_err());
        }
        assert!(Title::try_from(" ".to_owned()).is_err());
        assert!(Title::try_from("x".repeat(101)).is_err());
        assert!(Title::try_from("a\nb".to_owned()).is_err());
        assert!(Title::try_from("📝".repeat(100)).is_ok());
        assert!(NoteBody::try_from("📝".repeat(4000)).is_ok());
        assert!(NoteBody::try_from("x".repeat(4001)).is_err());
        assert!(NoteBody::try_from("a\0b".to_owned()).is_err());
        assert_eq!(
            NoteBody::try_from("a\r\nb\t".to_owned()).map(|b| b.0),
            Ok("a\nb\t".to_owned())
        );
        assert_eq!(
            NoteContent::try_from((String::new(), "\0".to_owned())),
            Err(FieldErrors {
                title: Some(ParseError::Title),
                body: Some(ParseError::Body)
            })
        );
        assert!(NotebookKey::try_from("a".repeat(64)).is_ok());
        assert!(NotebookKey::try_from("A".repeat(64)).is_err());
        assert!(NotebookKey::try_from("a".repeat(63)).is_err());
        assert!(NotebookKey::try_from("f".repeat(64)).is_ok());
        assert!(NotebookKey::try_from("0".repeat(64)).is_ok());
        assert!(NotebookKey::try_from("g".repeat(64)).is_err());
        assert!(NotebookKey::try_from("/".repeat(64)).is_err());
        assert!(NotebookKey::try_from(":".repeat(64)).is_err());
        assert!(NotebookKey::try_from("a".repeat(65)).is_err());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, rng_seed: proptest::test_runner::RngSeed::Fixed(731), ..ProptestConfig::default() })]
        #[test]
        fn accepted_titles_obey_the_contract(raw in any::<String>()) {
            let expected = (1..=100).contains(&raw.trim().chars().count()) && !raw.trim().chars().any(char::is_control);
            let normalized = raw.trim().to_owned();
            prop_assert_eq!(Title::try_from(raw.clone()).is_ok(), expected);
            if let Ok(title) = Title::try_from(raw) { prop_assert_eq!(title.as_str(), normalized); }
        }
        #[test]
        fn accepted_bodies_obey_the_contract(raw in any::<String>()) {
            let normalized = raw.replace("\r\n", "\n");
            let expected = normalized.chars().count() <= 4000 && !normalized.chars().any(|c| c.is_control() && c != '\n' && c != '\t');
            let body = NoteBody::try_from(raw);
            prop_assert_eq!(body.is_ok(), expected);
            if let Ok(body) = body { prop_assert_eq!(body.as_str(), normalized); }
        }
        #[test]
        fn positive_identifiers_round_trip(value in 1i64..=i64::MAX) {
            prop_assert_eq!(NoteId::try_from(value.to_string()).map(NoteId::value), Ok(value));
        }
        #[test]
        fn canonical_capabilities_round_trip_and_redact(bytes in prop::collection::vec(any::<u8>(), 32)) {
            let raw: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            let key = NotebookKey::try_from(raw.clone());
            prop_assert!(key.is_ok());
            if let Ok(key) = key {
                prop_assert_eq!(key.expose(), raw.as_str());
                let debug = format!("{key:?}");
                prop_assert_eq!(debug.as_str(), "NotebookKey([REDACTED])");
                prop_assert!(!debug.contains(raw.as_str()));
            }
        }
    }
}

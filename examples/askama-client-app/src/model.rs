//! Pure parsing of the example's public page parameters.
use serde::Deserialize;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum InputError {
    #[error("use 2–64 non-control characters for your name")]
    Name,
    #[error("use a starting count from 0–99")]
    Count,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VisitorName(String);

impl TryFrom<String> for VisitorName {
    type Error = InputError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let name = raw.trim();
        if !(2..=64).contains(&name.chars().count()) || name.chars().any(char::is_control) {
            return Err(InputError::Name);
        }
        Ok(Self(name.to_owned()))
    }
}

impl VisitorName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StartingCount(u16);

impl TryFrom<String> for StartingCount {
    type Error = InputError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let count = raw.parse::<u16>().map_err(|_| InputError::Count)?;
        if count > 99 {
            return Err(InputError::Count);
        }
        Ok(Self(count))
    }
}

impl StartingCount {
    pub(crate) const fn value(&self) -> u16 {
        self.0
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawPageParams {
    name: Option<String>,
    start: Option<String>,
}

#[derive(Debug)]
pub(crate) struct PageParams {
    pub(crate) name: VisitorName,
    pub(crate) start: StartingCount,
}

impl TryFrom<RawPageParams> for PageParams {
    type Error = InputError;

    fn try_from(raw: RawPageParams) -> Result<Self, Self::Error> {
        Ok(Self {
            name: raw.name.unwrap_or_else(|| "friend".to_owned()).try_into()?,
            start: raw.start.unwrap_or_else(|| "5".to_owned()).try_into()?,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_invalid_names_and_count_boundaries() {
        for name in ["", "x", "   ", "a\nb"] {
            assert_eq!(
                VisitorName::try_from(name.to_owned()),
                Err(InputError::Name)
            );
        }
        assert_eq!(VisitorName::try_from("x".repeat(65)), Err(InputError::Name));
        for count in ["-1", "100", "65536", "NaN", "", "2.5"] {
            assert_eq!(
                StartingCount::try_from(count.to_owned()),
                Err(InputError::Count)
            );
        }
        for name in ["xy".to_owned(), "🏀".repeat(64)] {
            assert_eq!(
                VisitorName::try_from(name.clone())
                    .expect("invariant: boundary name is valid")
                    .as_str(),
                name
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, rng_seed: proptest::test_runner::RngSeed::Fixed(731), ..ProptestConfig::default() })]
        #[test]
        fn trimming_preserves_valid_unicode_names(name in "[A-Za-zé🏀]{2,64}") {
            let parsed = VisitorName::try_from(format!("  {name}  ")).expect("invariant: generator emits valid names");
            prop_assert_eq!(parsed.as_str(), name.as_str());
            prop_assert_eq!(VisitorName::try_from(parsed.as_str().to_owned()), Ok(parsed));
        }

        #[test]
        fn starting_counts_round_trip(count in 0u16..100) {
            let parsed = StartingCount::try_from(count.to_string()).expect("invariant: generator emits 0–99");
            prop_assert_eq!(parsed.value(), count);
            prop_assert_eq!(StartingCount::try_from(parsed.value().to_string()), Ok(parsed));
        }
    }
}

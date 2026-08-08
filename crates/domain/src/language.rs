use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LanguageError {
    #[error("'{0}' is not a supported language")]
    Unsupported(String),
}

/// The interface languages the app ships with.
///
/// Deliberately a closed enum rather than an open BCP-47 tag: every string the
/// UI can render exists in exactly these variants, so a tag nothing has been
/// translated into is not a language the app can honor. Storing `"de"` because
/// the OS reported it would leave the user with a setting that claims German
/// and renders English.
///
/// This is *not* the same axis as `Currency`. Currency relabels amounts;
/// language relabels the app. A French user with a USD account is an ordinary
/// case, so the two settings never derive from one another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    En,
    Fr,
}

impl Language {
    /// Every variant, for exhaustive iteration in tests and in the settings
    /// picker. The `match` in `as_str` is what keeps this honest — adding a
    /// variant without adding it here stops compiling.
    pub const ALL: [Language; 2] = [Language::En, Language::Fr];

    /// The stored/wire spelling. This is the exact string that goes into the
    /// `settings` table and across the IPC boundary, so changing one is a
    /// migration, not a rename.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, LanguageError> {
        match raw {
            "en" => Ok(Self::En),
            "fr" => Ok(Self::Fr),
            other => Err(LanguageError::Unsupported(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_survives_a_string_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::parse(language.as_str()).unwrap(), language);
        }
    }

    /// The wire contract with the frontend: `Language` in
    /// `frontend/src/lib/i18n.svelte.ts` is a string union sent straight into
    /// `parse` with no translation step, so these spellings are load-bearing.
    #[test]
    fn language_spellings_match_the_frontend_union() {
        assert_eq!(Language::En.as_str(), "en");
        assert_eq!(Language::Fr.as_str(), "fr");
    }

    /// English is what a database with no language row renders as — the
    /// setting is introduced by a migration that backfills nothing, so every
    /// existing database must land somewhere sensible without being asked.
    #[test]
    fn the_default_language_is_english() {
        assert_eq!(Language::default(), Language::En);
    }

    #[test]
    fn an_untranslated_language_is_rejected_and_named() {
        let error = Language::parse("de").unwrap_err();
        assert!(
            error.to_string().contains("de"),
            "the error should quote the offending input, got: {error}"
        );
        assert!(Language::parse("EN").is_err());
        assert!(Language::parse("en-GB").is_err());
        assert!(Language::parse("").is_err());
    }
}

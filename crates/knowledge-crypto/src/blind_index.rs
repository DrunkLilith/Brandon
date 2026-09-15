use std::collections::BTreeMap;

use unicode_normalization::UnicodeNormalization;

use crate::PurposeKey;

const TOKEN_DOMAIN: &[u8] = b"knowledge-platform/blind-index/v1\0";
const MIN_PREFIX_LENGTH: usize = 3;
const MAX_PREFIX_LENGTH: usize = 12;

/// A keyed, non-reversible search token suitable for the blind-index table.
#[derive(Clone, Eq, PartialEq)]
pub struct BlindToken {
    hmac: [u8; 32],
    prefix_length: u8,
    frequency: u32,
}

impl std::fmt::Debug for BlindToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BlindToken")
            .field("hmac", &"[redacted]")
            .field("prefix_length", &self.prefix_length)
            .field("frequency", &self.frequency)
            .finish()
    }
}

impl BlindToken {
    #[must_use]
    pub const fn hmac(&self) -> &[u8; 32] {
        &self.hmac
    }

    #[must_use]
    pub const fn prefix_length(&self) -> u8 {
        self.prefix_length
    }

    #[must_use]
    pub const fn frequency(&self) -> u32 {
        self.frequency
    }
}

/// Normalizes text and turns full words plus bounded prefixes into keyed HMACs.
pub struct BlindIndexer {
    search_key: PurposeKey,
}

impl BlindIndexer {
    #[must_use]
    pub const fn new(search_key: PurposeKey) -> Self {
        Self { search_key }
    }

    /// Creates deterministic HMAC tokens. `language` is accepted to make the
    /// language-aware tokenization boundary explicit; v1 uses the same Unicode
    /// normalization for Russian and English and has no stemming.
    #[must_use]
    pub fn tokens(&self, language: &str, text: &str) -> Vec<BlindToken> {
        let mut counts = BTreeMap::<(u8, [u8; 32]), u32>::new();
        for word in Self::words(language, text) {
            self.add_token(&mut counts, &word, 0);
            let character_count = word.chars().count();
            for prefix_length in MIN_PREFIX_LENGTH..=MAX_PREFIX_LENGTH.min(character_count) {
                let prefix: String = word.chars().take(prefix_length).collect();
                self.add_token(&mut counts, &prefix, prefix_length as u8);
            }
        }
        counts
            .into_iter()
            .map(|((prefix_length, hmac), frequency)| BlindToken {
                hmac,
                prefix_length,
                frequency,
            })
            .collect()
    }

    /// Returns normalized word candidates for in-memory phrase verification.
    /// The caller must keep these values out of persistent storage and logs.
    #[must_use]
    pub fn words(_language: &str, text: &str) -> Vec<String> {
        let normalized: String = text.nfkc().flat_map(char::to_lowercase).collect();
        normalized
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn add_token(
        &self,
        counts: &mut BTreeMap<(u8, [u8; 32]), u32>,
        value: &str,
        prefix_length: u8,
    ) {
        let mut authenticated = Vec::with_capacity(TOKEN_DOMAIN.len() + value.len());
        authenticated.extend_from_slice(TOKEN_DOMAIN);
        authenticated.extend_from_slice(value.as_bytes());
        let hmac = self.search_key.authenticate(&authenticated);
        *counts.entry((prefix_length, hmac)).or_default() += 1;
    }
}

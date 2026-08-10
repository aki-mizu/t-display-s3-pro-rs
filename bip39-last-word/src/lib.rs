#![no_std]
#![forbid(unsafe_code)]

//! Allocation-free BIP39 final-word calculation for a 12-word mnemonic.
//!
//! Eleven BIP39 words contain 121 of the 128 entropy bits. The remaining seven
//! entropy bits must be supplied explicitly; each choice produces one of the
//! 128 valid final-word candidates. This crate never selects those seven bits
//! on the caller's behalf.

use bip39::Language;
use bitcoin_hashes::{Hash, sha256};

/// Number of known words in a 12-word BIP39 mnemonic.
pub const PREFIX_WORD_COUNT: usize = 11;

/// Number of possible final words for eleven known words.
pub const CANDIDATE_COUNT: usize = 128;

const WORD_LIST_SIZE: u16 = 2048;
const CHECKSUM_BITS: u32 = 4;
const UNKNOWN_ENTROPY_BITS: u32 = 7;
const MAX_ENTROPY_BITS: u8 = 0b0111_1111;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PinyinEntry {
    spelling: &'static str,
    index: u16,
}

include!(concat!(env!("OUT_DIR"), "/simplified_chinese_pinyin.rs"));

/// Allocation-free pinyin matches for the Simplified Chinese BIP39 word list.
///
/// Pinyin is only an input aid. Resolve a returned index to its canonical
/// Chinese BIP39 character before accepting it as a mnemonic word.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimplifiedChinesePinyinMatches {
    entries: &'static [PinyinEntry],
}

impl SimplifiedChinesePinyinMatches {
    /// Returns the number of distinct BIP39 words matched by the pinyin prefix.
    pub fn len(&self) -> usize {
        (0..self.entries.len())
            .filter(|position| self.is_first_entry_for_index(*position))
            .count()
    }

    /// Returns whether the pinyin prefix matches no BIP39 words.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the BIP39 word index at the distinct candidate position.
    pub fn word_index_at(&self, candidate_position: usize) -> Option<u16> {
        let mut distinct_position = 0;
        for (entry_position, entry) in self.entries.iter().enumerate() {
            if !self.is_first_entry_for_index(entry_position) {
                continue;
            }
            if distinct_position == candidate_position {
                return Some(entry.index);
            }
            distinct_position += 1;
        }

        None
    }

    fn is_first_entry_for_index(&self, entry_position: usize) -> bool {
        let index = self.entries[entry_position].index;
        !self.entries[..entry_position]
            .iter()
            .any(|entry| entry.index == index)
    }
}

/// Returns the Simplified Chinese BIP39 words whose tone-free pinyin starts with `prefix`.
///
/// Pinyin aliases such as `v` for umlaut-u are normalized while the static table
/// is generated. Each Chinese character occurs at most once in the result even
/// when it has multiple pronunciations.
pub fn simplified_chinese_words_by_pinyin_prefix(prefix: &str) -> SimplifiedChinesePinyinMatches {
    if !prefix.bytes().all(|byte| byte.is_ascii_lowercase()) {
        return SimplifiedChinesePinyinMatches { entries: &[] };
    }

    let Some(first) = SIMPLIFIED_CHINESE_PINYIN_ENTRIES
        .iter()
        .position(|entry| entry.spelling.starts_with(prefix))
    else {
        return SimplifiedChinesePinyinMatches { entries: &[] };
    };
    let count = SIMPLIFIED_CHINESE_PINYIN_ENTRIES[first..]
        .iter()
        .take_while(|entry| entry.spelling.starts_with(prefix))
        .count();

    SimplifiedChinesePinyinMatches {
        entries: &SIMPLIFIED_CHINESE_PINYIN_ENTRIES[first..first + count],
    }
}

/// Returns the pinyin letters that can continue a Simplified Chinese BIP39 prefix.
pub fn simplified_chinese_next_pinyin_letters(prefix: &str) -> [bool; 26] {
    let mut enabled = [false; 26];

    for entry in simplified_chinese_words_by_pinyin_prefix(prefix).entries {
        let Some(next) = entry.spelling.as_bytes().get(prefix.len()).copied() else {
            continue;
        };
        if next.is_ascii_lowercase() {
            enabled[usize::from(next - b'a')] = true;
        }
    }

    enabled
}

/// BIP39 word lists supported by the final-word helper.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MnemonicLanguage {
    /// The English BIP39 word list.
    #[default]
    English,
    /// The Simplified Chinese BIP39 word list.
    SimplifiedChinese,
}

impl MnemonicLanguage {
    fn bip39_language(self) -> Language {
        match self {
            Self::English => Language::English,
            Self::SimplifiedChinese => Language::SimplifiedChinese,
        }
    }
}

/// Input validation failure while calculating a final BIP39 word.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// A word is not an exact BIP39 word in the selected word list.
    UnknownWord { position: usize },
    /// A caller-provided BIP39 word index is outside the BIP39 word list.
    InvalidWordIndex { position: usize, index: u16 },
    /// The seven remaining entropy bits must be in the range `0..=127`.
    InvalidEntropyBits { value: u8 },
}

/// The 128 valid BIP39 final-word candidates associated with one 11-word prefix.
///
/// Candidate position `n` corresponds to the explicit seven-bit entropy value
/// `n`; use [`Self::word_for_entropy_bits`] to retrieve its English word.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LastWordCandidates {
    word_indices: [u16; CANDIDATE_COUNT],
}

impl LastWordCandidates {
    /// Returns the number of candidates, always [`CANDIDATE_COUNT`].
    pub const fn len(&self) -> usize {
        CANDIDATE_COUNT
    }

    /// Candidate sets are never empty.
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the BIP39 indices for all candidates.
    pub fn word_indices(&self) -> &[u16; CANDIDATE_COUNT] {
        &self.word_indices
    }

    /// Returns the candidate's BIP39 word index for the given seven bits.
    pub fn word_index_for_entropy_bits(&self, entropy_bits: u8) -> Result<u16, Error> {
        self.word_indices
            .get(usize::from(entropy_bits))
            .copied()
            .ok_or(Error::InvalidEntropyBits {
                value: entropy_bits,
            })
    }

    /// Returns the candidate's English BIP39 word for the given seven bits.
    pub fn word_for_entropy_bits(&self, entropy_bits: u8) -> Result<&'static str, Error> {
        let index = self.word_index_for_entropy_bits(entropy_bits)?;
        word_for_index(index).ok_or(Error::InvalidWordIndex {
            position: PREFIX_WORD_COUNT,
            index,
        })
    }
}

/// Resolves a BIP39 word index to its word in the requested language.
pub fn word_for_index_in(language: MnemonicLanguage, index: u16) -> Option<&'static str> {
    language
        .bip39_language()
        .word_list()
        .get(usize::from(index))
        .copied()
}

/// Resolves an exact lowercase English BIP39 word to its index.
pub fn word_index(word: &str) -> Option<u16> {
    Language::English.find_word(word)
}

/// Resolves an English BIP39 word index to its word.
pub fn word_for_index(index: u16) -> Option<&'static str> {
    Language::English
        .word_list()
        .get(usize::from(index))
        .copied()
}

/// Returns the contiguous English BIP39 word-list range that begins with `prefix`.
///
/// The returned words are static data and do not allocate. An empty prefix returns
/// the complete English word list.
pub fn words_by_prefix(prefix: &str) -> &'static [&'static str] {
    let words = Language::English.word_list();
    let Some(first) = words.iter().position(|word| word.starts_with(prefix)) else {
        return &[];
    };
    let count = words[first..]
        .iter()
        .take_while(|word| word.starts_with(prefix))
        .count();

    &words[first..first + count]
}

/// Converts exactly eleven English BIP39 words to word-list indices.
pub fn indices_from_words(
    words: &[&str; PREFIX_WORD_COUNT],
) -> Result<[u16; PREFIX_WORD_COUNT], Error> {
    let mut indices = [0; PREFIX_WORD_COUNT];

    for (position, word) in words.iter().enumerate() {
        indices[position] = word_index(word).ok_or(Error::UnknownWord { position })?;
    }

    Ok(indices)
}

/// Calculates all valid final-word candidates from eleven English BIP39 words.
pub fn candidates_from_words(
    words: &[&str; PREFIX_WORD_COUNT],
) -> Result<LastWordCandidates, Error> {
    candidates_from_indices(&indices_from_words(words)?)
}

/// Calculates all valid final-word candidates from eleven English BIP39 indices.
pub fn candidates_from_indices(
    indices: &[u16; PREFIX_WORD_COUNT],
) -> Result<LastWordCandidates, Error> {
    let prefix = entropy_prefix(indices)?;
    let mut word_indices = [0; CANDIDATE_COUNT];

    for entropy_bits in 0..=MAX_ENTROPY_BITS {
        word_indices[usize::from(entropy_bits)] = final_word_index(prefix, entropy_bits);
    }

    Ok(LastWordCandidates { word_indices })
}

/// Calculates one final word in the requested language from eleven BIP39 indices and seven entropy bits.
pub fn word_for_entropy_bits_in(
    language: MnemonicLanguage,
    indices: &[u16; PREFIX_WORD_COUNT],
    entropy_bits: u8,
) -> Result<&'static str, Error> {
    let prefix = entropy_prefix(indices)?;
    validate_entropy_bits(entropy_bits)?;

    let index = final_word_index(prefix, entropy_bits);
    word_for_index_in(language, index).ok_or(Error::InvalidWordIndex {
        position: PREFIX_WORD_COUNT,
        index,
    })
}

/// Calculates one final English word from eleven BIP39 indices and seven entropy bits.
pub fn word_for_entropy_bits(
    indices: &[u16; PREFIX_WORD_COUNT],
    entropy_bits: u8,
) -> Result<&'static str, Error> {
    word_for_entropy_bits_in(MnemonicLanguage::English, indices, entropy_bits)
}

fn entropy_prefix(indices: &[u16; PREFIX_WORD_COUNT]) -> Result<u128, Error> {
    let mut prefix = 0_u128;

    for (position, index) in indices.iter().copied().enumerate() {
        if index >= WORD_LIST_SIZE {
            return Err(Error::InvalidWordIndex { position, index });
        }

        prefix = (prefix << 11) | u128::from(index);
    }

    Ok(prefix)
}

fn validate_entropy_bits(entropy_bits: u8) -> Result<(), Error> {
    if entropy_bits > MAX_ENTROPY_BITS {
        return Err(Error::InvalidEntropyBits {
            value: entropy_bits,
        });
    }

    Ok(())
}

fn final_word_index(prefix: u128, entropy_bits: u8) -> u16 {
    // `to_be_bytes()` always yields all 16 entropy bytes. This is essential:
    // hashing a variable-width integer would discard leading zero bytes and
    // generate an invalid checksum (e.g. `abandon` x11 + 0000000 -> `about`).
    let entropy = (prefix << UNKNOWN_ENTROPY_BITS) | u128::from(entropy_bits);
    let checksum = sha256::Hash::hash(&entropy.to_be_bytes())[0] >> (8 - CHECKSUM_BITS);

    (u16::from(entropy_bits) << CHECKSUM_BITS) | u16::from(checksum)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABANDON: [&str; PREFIX_WORD_COUNT] = ["abandon"; PREFIX_WORD_COUNT];

    #[test]
    fn leading_zero_entropy_uses_all_sixteen_bytes() {
        let indices = indices_from_words(&ABANDON).expect("known BIP39 words");

        assert_eq!(word_for_entropy_bits(&indices, 0), Ok("about"));
    }

    #[test]
    fn matches_official_seven_f_entropy_vector() {
        let words = [
            "legal", "winner", "thank", "year", "wave", "sausage", "worth", "useful", "legal",
            "winner", "thank",
        ];
        let indices = indices_from_words(&words).expect("known BIP39 words");

        assert_eq!(word_for_entropy_bits(&indices, 127), Ok("yellow"));
    }

    #[test]
    fn matches_official_all_one_entropy_vector() {
        assert_eq!(
            word_for_entropy_bits(&[2047; PREFIX_WORD_COUNT], 127),
            Ok("wrong")
        );
    }

    #[test]
    fn candidates_are_unique_and_match_the_direct_calculation() {
        let indices = indices_from_words(&ABANDON).expect("known BIP39 words");
        let candidates = candidates_from_indices(&indices).expect("valid BIP39 word indices");
        let candidate_indices = candidates.word_indices();

        assert_eq!(candidates.len(), CANDIDATE_COUNT);
        for entropy_bits in 0..=MAX_ENTROPY_BITS {
            let candidate_index = candidates
                .word_index_for_entropy_bits(entropy_bits)
                .expect("entropy bits are in range");

            assert_eq!(candidate_index >> CHECKSUM_BITS, u16::from(entropy_bits));
            assert!(!candidate_indices[..usize::from(entropy_bits)].contains(&candidate_index));
            assert_eq!(
                candidates.word_for_entropy_bits(entropy_bits),
                word_for_entropy_bits(&indices, entropy_bits)
            );
        }
    }

    #[test]
    fn rejects_unknown_words_and_invalid_inputs() {
        let mut words = ABANDON;
        words[4] = "not-a-bip39-word";
        assert_eq!(
            indices_from_words(&words),
            Err(Error::UnknownWord { position: 4 })
        );

        let mut indices = [0; PREFIX_WORD_COUNT];
        indices[7] = WORD_LIST_SIZE;
        assert_eq!(
            candidates_from_indices(&indices),
            Err(Error::InvalidWordIndex {
                position: 7,
                index: WORD_LIST_SIZE,
            })
        );
        assert_eq!(
            word_for_entropy_bits(&[0; PREFIX_WORD_COUNT], 128),
            Err(Error::InvalidEntropyBits { value: 128 })
        );
    }

    #[test]
    fn finds_word_prefixes_without_allocating() {
        assert_eq!(words_by_prefix("aban"), ["abandon"]);
        assert!(words_by_prefix("not-a-bip39-prefix").is_empty());
        assert!(words_by_prefix("act").contains(&"act"));
    }

    #[test]
    fn resolves_simplified_chinese_words_and_pinyin() {
        let language = MnemonicLanguage::SimplifiedChinese;
        assert_eq!(word_for_index_in(language, 0), Some("\u{7684}"));

        let de_matches = simplified_chinese_words_by_pinyin_prefix("de");
        assert!(
            (0..de_matches.len()).any(|position| de_matches.word_index_at(position) == Some(0))
        );

        let xing_matches = simplified_chinese_words_by_pinyin_prefix("xing");
        assert!(
            (0..xing_matches.len())
                .any(|position| xing_matches.word_index_at(position) == Some(56))
        );
        let hang_matches = simplified_chinese_words_by_pinyin_prefix("hang");
        assert!(
            (0..hang_matches.len())
                .any(|position| hang_matches.word_index_at(position) == Some(56))
        );

        assert_eq!(
            word_for_entropy_bits_in(language, &[0; PREFIX_WORD_COUNT], 0),
            Ok("\u{5728}")
        );
    }
}

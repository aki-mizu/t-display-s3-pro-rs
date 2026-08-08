#![no_std]
#![forbid(unsafe_code)]

//! Allocation-free BIP39 final-word calculation for an English 12-word mnemonic.
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

/// Input validation failure while calculating a final BIP39 word.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// A word is not an exact lowercase English BIP39 word.
    UnknownWord { position: usize },
    /// A caller-provided BIP39 word index is outside the English word list.
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

    /// Returns the English BIP39 indices for all candidates.
    pub fn word_indices(&self) -> &[u16; CANDIDATE_COUNT] {
        &self.word_indices
    }

    /// Returns the candidate's English BIP39 word index for the given seven bits.
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

/// Calculates one final word from eleven English BIP39 indices and seven entropy bits.
pub fn word_for_entropy_bits(
    indices: &[u16; PREFIX_WORD_COUNT],
    entropy_bits: u8,
) -> Result<&'static str, Error> {
    let prefix = entropy_prefix(indices)?;
    validate_entropy_bits(entropy_bits)?;

    let index = final_word_index(prefix, entropy_bits);
    word_for_index(index).ok_or(Error::InvalidWordIndex {
        position: PREFIX_WORD_COUNT,
        index,
    })
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
}

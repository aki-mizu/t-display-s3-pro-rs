use bip39_last_word::{
    Error as Bip39Error, PREFIX_WORD_COUNT, word_for_index, word_index, words_by_prefix,
};

use super::{CommitFeedback, LastWordFlow, MAX_WORD_PREFIX_LEN};

impl LastWordFlow {
    pub(crate) fn push_letter(&mut self, key: i32) {
        if self.is_complete() || self.prefix_len == MAX_WORD_PREFIX_LEN {
            return;
        }

        let Ok(letter_offset) = u8::try_from(key) else {
            return;
        };
        if letter_offset >= 26 {
            return;
        }

        let letter = b'a' + letter_offset;
        let mut candidate_prefix = self.prefix;
        candidate_prefix[self.prefix_len] = letter;
        let candidate_len = self.prefix_len + 1;
        let Ok(candidate) = core::str::from_utf8(&candidate_prefix[..candidate_len]) else {
            return;
        };

        if words_by_prefix(candidate).is_empty() {
            // Do not add an impossible spelling to the prefix, but make the
            // deliberate BIP39 filter visible so it is not mistaken for a
            // missed touchscreen tap.
            self.last_letter_was_rejected = true;
            return;
        }

        self.prefix = candidate_prefix;
        self.prefix_len = candidate_len;
        self.last_letter_was_rejected = false;
        self.commit_feedback = CommitFeedback::None;
    }

    pub(crate) fn backspace(&mut self) {
        self.last_letter_was_rejected = false;
        self.commit_feedback = CommitFeedback::None;

        if self.prefix_len > 0 {
            self.prefix_len -= 1;
            self.prefix[self.prefix_len] = 0;
            return;
        }

        if self.word_count > 0 {
            self.word_count -= 1;
            self.word_indices[self.word_count] = 0;
            self.clear_entropy();
        }
    }

    pub(crate) fn commit_word(&mut self) -> bool {
        if self.is_complete() {
            return true;
        }

        let prefix = self.prefix_text();
        if prefix.is_empty() {
            self.commit_feedback = CommitFeedback::EnterLetters;
            return false;
        }

        let matches = words_by_prefix(prefix);
        let Some(index) = self.selected_word_index() else {
            self.commit_feedback = CommitFeedback::MoreMatches {
                count: matches.len(),
            };
            return false;
        };

        self.word_indices[self.word_count] = index;
        self.word_count += 1;
        self.prefix = [0; MAX_WORD_PREFIX_LEN];
        self.prefix_len = 0;
        self.last_letter_was_rejected = false;
        self.commit_feedback = CommitFeedback::None;
        self.clear_entropy();

        self.is_complete()
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Replaces the complete eleven-word prefix only after every index has
    /// been checked, so an invalid board input cannot partially change it.
    pub(crate) fn load_word_indices(
        &mut self,
        word_indices: [u16; PREFIX_WORD_COUNT],
    ) -> Result<(), Bip39Error> {
        for (position, index) in word_indices.iter().copied().enumerate() {
            if word_for_index(index).is_none() {
                return Err(Bip39Error::InvalidWordIndex { position, index });
            }
        }

        self.word_indices = word_indices;
        self.word_count = PREFIX_WORD_COUNT;
        self.prefix = [0; MAX_WORD_PREFIX_LEN];
        self.prefix_len = 0;
        self.commit_feedback = CommitFeedback::None;
        self.clear_entropy();
        Ok(())
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.word_count == PREFIX_WORD_COUNT
    }

    pub(crate) fn prefix_text(&self) -> &str {
        core::str::from_utf8(&self.prefix[..self.prefix_len]).unwrap_or("")
    }

    pub(crate) fn selected_word_index(&self) -> Option<u16> {
        let prefix = self.prefix_text();
        if prefix.is_empty() {
            return None;
        }

        if let Some(index) = word_index(prefix) {
            return Some(index);
        }

        let matches = words_by_prefix(prefix);
        (matches.len() == 1)
            .then(|| word_index(matches[0]))
            .flatten()
    }

    /// Returns the only letters that can extend the current entry to an
    /// English BIP39 word. This is the same prefix filtering interaction used
    /// by Jade: impossible keys are disabled before they can be tapped.
    pub(crate) fn next_letter_enabled(&self) -> [bool; 26] {
        let mut enabled = [false; 26];

        // The word list's first four letters identify every English BIP39
        // word. Once a word can be confirmed, leave letter entry closed until
        // the user confirms it or backspaces.
        if self.is_complete() || self.prefix_len == MAX_WORD_PREFIX_LEN {
            return enabled;
        }

        for word in words_by_prefix(self.prefix_text()) {
            let Some(next) = word.as_bytes().get(self.prefix_len).copied() else {
                // An exact short word can be committed, but does not enable a
                // further character by itself.
                continue;
            };
            if next.is_ascii_lowercase() {
                enabled[usize::from(next - b'a')] = true;
            }
        }

        enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_exact_short_words_and_keeps_indices_only() {
        let mut flow = LastWordFlow::default();
        for byte in b"act" {
            flow.push_letter(i32::from(*byte - b'a'));
        }

        assert_eq!(flow.selected_word_index(), word_index("act"));
        assert!(!flow.commit_word());
        assert_eq!(flow.word_indices[0], word_index("act").unwrap());
    }

    #[test]
    fn makes_an_impossible_next_letter_explicit() {
        let mut flow = LastWordFlow::default();
        flow.push_letter(0); // a
        flow.push_letter(0); // aa is not a BIP39 word prefix

        assert_eq!(flow.prefix_text(), "a");
        assert!(flow.last_letter_was_rejected);
        assert!(flow.message().contains("No BIP39 word"));

        flow.push_letter(1); // ab is a valid prefix, for example "abandon"
        assert_eq!(flow.prefix_text(), "ab");
        assert!(!flow.last_letter_was_rejected);
    }

    #[test]
    fn enables_only_letters_that_can_continue_the_current_prefix() {
        let mut flow = LastWordFlow::default();

        // At the start of a word, only BIP39 initial letters are offered.
        let initial = flow.next_letter_enabled();
        assert!(initial[usize::from(b'a' - b'a')]);
        assert!(!initial[usize::from(b'x' - b'a')]);

        // "aba" can only be continued as "aban..." in the English list.
        for byte in b"aba" {
            flow.push_letter(i32::from(*byte - b'a'));
        }
        let after_aba = flow.next_letter_enabled();
        assert!(after_aba[usize::from(b'n' - b'a')]);
        assert_eq!(after_aba.into_iter().filter(|enabled| *enabled).count(), 1);

        // Four letters are sufficient to identify a BIP39 word, so the UI
        // stops accepting more letters and makes the user confirm or edit.
        flow.push_letter(i32::from(b'n' - b'a'));
        assert!(
            flow.next_letter_enabled()
                .into_iter()
                .all(|enabled| !enabled)
        );
    }

    #[test]
    fn use_word_explains_an_unfinished_or_ambiguous_entry() {
        let mut flow = LastWordFlow::default();

        assert!(!flow.commit_word());
        assert_eq!(flow.commit_feedback, CommitFeedback::EnterLetters);
        assert!(flow.message().contains("Enter a BIP39 word"));

        flow.push_letter(0); // a has many BIP39 matches
        assert!(!flow.commit_word());
        assert!(matches!(
            flow.commit_feedback,
            CommitFeedback::MoreMatches { count } if count > 1
        ));
        assert!(flow.message().contains("matching words remain"));
    }

    #[test]
    fn the_eleventh_confirmation_completes_the_prefix() {
        let mut flow = LastWordFlow::default();
        for _ in 0..(PREFIX_WORD_COUNT - 1) {
            super::super::enter_word(&mut flow, "abandon");
        }

        for byte in b"aban" {
            flow.push_letter(i32::from(*byte - b'a'));
        }
        assert!(flow.commit_word());
        assert!(flow.is_complete());
    }

    #[test]
    fn loading_indices_replaces_the_prefix_and_clears_entropy() {
        let mut flow = LastWordFlow::default();
        flow.word_indices[0] = 123;
        flow.word_count = 1;
        flow.prefix[..3].copy_from_slice(b"act");
        flow.prefix_len = 3;
        flow.entropy_bits = 0b101_0101;
        flow.entropy_confirmed = true;

        let indices = core::array::from_fn(|position| position as u16);
        assert_eq!(flow.load_word_indices(indices), Ok(()));

        assert_eq!(flow.word_indices, indices);
        assert!(flow.is_complete());
        assert_eq!(flow.prefix_text(), "");
        assert_eq!(flow.entropy_bits, 0);
        assert!(!flow.entropy_confirmed);
        assert_eq!(flow.candidate_word(), None);
    }

    #[test]
    fn invalid_loaded_index_does_not_partially_replace_the_prefix() {
        let mut flow = LastWordFlow::default();
        flow.word_indices[0] = 123;
        flow.word_count = 1;
        flow.prefix[..3].copy_from_slice(b"act");
        flow.prefix_len = 3;

        let mut indices = [0; PREFIX_WORD_COUNT];
        indices[7] = 2048;
        assert_eq!(
            flow.load_word_indices(indices),
            Err(Bip39Error::InvalidWordIndex {
                position: 7,
                index: 2048,
            })
        );

        assert_eq!(flow.word_indices[0], 123);
        assert_eq!(flow.word_count, 1);
        assert_eq!(flow.prefix_text(), "act");
    }
}

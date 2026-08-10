use alloc::{string::String, vec::Vec};
use bip39_last_word::{
    Error as Bip39Error, MnemonicLanguage, PREFIX_WORD_COUNT,
    simplified_chinese_next_pinyin_letters, simplified_chinese_words_by_pinyin_prefix,
    word_for_index_in, word_index, words_by_prefix,
};

use super::{
    CommitFeedback, LastWordFlow, MAX_ENGLISH_PREFIX_LEN, MAX_WORD_PREFIX_LEN,
    PINYIN_CANDIDATES_PER_PAGE,
};

impl LastWordFlow {
    pub(crate) fn select_language(&mut self, language: i32) {
        let language = match language {
            0 => MnemonicLanguage::English,
            1 => MnemonicLanguage::SimplifiedChinese,
            _ => return,
        };
        if self.language != language {
            *self = Self {
                language,
                ..Self::default()
            };
        }
        self.language_selected = true;
    }

    pub(crate) fn language_index(&self) -> i32 {
        match self.language {
            MnemonicLanguage::English => 0,
            MnemonicLanguage::SimplifiedChinese => 1,
        }
    }

    pub(crate) fn is_language_selection_visible(&self) -> bool {
        !self.language_selected
    }

    pub(crate) fn push_letter(&mut self, key: i32) {
        if self.is_complete() || self.prefix_len == self.prefix_len_limit() {
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

        if !self.prefix_has_matches(candidate) {
            // Do not add an impossible spelling to the prefix, but make the
            // deliberate BIP39 filter visible so it is not mistaken for a
            // missed touchscreen tap.
            self.last_letter_was_rejected = true;
            return;
        }

        self.prefix = candidate_prefix;
        self.prefix_len = candidate_len;
        self.language_selected = true;
        self.last_letter_was_rejected = false;
        self.commit_feedback = CommitFeedback::None;
        self.pinyin_candidate_page = 0;
        self.pinyin_candidate_picker_open = false;
    }

    pub(crate) fn backspace(&mut self) {
        self.last_letter_was_rejected = false;
        self.commit_feedback = CommitFeedback::None;
        self.pinyin_candidate_page = 0;
        self.pinyin_candidate_picker_open = false;

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

        let Some(index) = self.selected_word_index() else {
            self.commit_feedback = CommitFeedback::MoreMatches {
                count: self.prefix_match_count(),
            };
            return false;
        };

        self.accept_word_index(index);
        self.is_complete()
    }

    pub(crate) fn reset(&mut self) {
        *self = Self {
            language: self.language,
            ..Self::default()
        };
    }

    /// Replaces the complete eleven-word prefix only after every index has
    /// been checked, so an invalid board input cannot partially change it.
    pub(crate) fn load_word_indices(
        &mut self,
        word_indices: [u16; PREFIX_WORD_COUNT],
    ) -> Result<(), Bip39Error> {
        for (position, index) in word_indices.iter().copied().enumerate() {
            if word_for_index_in(self.language, index).is_none() {
                return Err(Bip39Error::InvalidWordIndex { position, index });
            }
        }

        self.word_indices = word_indices;
        self.word_count = PREFIX_WORD_COUNT;
        self.language_selected = true;
        self.prefix = [0; MAX_WORD_PREFIX_LEN];
        self.prefix_len = 0;
        self.last_letter_was_rejected = false;
        self.commit_feedback = CommitFeedback::None;
        self.pinyin_candidate_page = 0;
        self.pinyin_candidate_picker_open = false;
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

        match self.language {
            MnemonicLanguage::English => {
                if let Some(index) = word_index(prefix) {
                    return Some(index);
                }

                let matches = words_by_prefix(prefix);
                (matches.len() == 1)
                    .then(|| word_index(matches[0]))
                    .flatten()
            }
            MnemonicLanguage::SimplifiedChinese => {
                let matches = simplified_chinese_words_by_pinyin_prefix(prefix);
                (matches.len() == 1)
                    .then(|| matches.word_index_at(0))
                    .flatten()
            }
        }
    }

    pub(crate) fn can_commit_or_choose(&self) -> bool {
        self.selected_word_index().is_some() || self.can_open_pinyin_candidate_picker()
    }

    /// Returns the only letters that can extend the current entry to a BIP39
    /// word. English uses word spelling; Simplified Chinese uses pinyin.
    pub(crate) fn next_letter_enabled(&self) -> [bool; 26] {
        // The word list's first four letters identify every English BIP39
        // word. Once a word can be confirmed, leave letter entry closed until
        // the user confirms it or backspaces.
        if self.is_complete() || self.prefix_len == self.prefix_len_limit() {
            return [false; 26];
        }

        match self.language {
            MnemonicLanguage::English => {
                let mut enabled = [false; 26];
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
            MnemonicLanguage::SimplifiedChinese => {
                simplified_chinese_next_pinyin_letters(self.prefix_text())
            }
        }
    }

    pub(crate) fn pinyin_candidate_labels(&self) -> Vec<String> {
        let mut labels = Vec::with_capacity(PINYIN_CANDIDATES_PER_PAGE);
        if self.language != MnemonicLanguage::SimplifiedChinese {
            labels.resize(PINYIN_CANDIDATES_PER_PAGE, String::new());
            return labels;
        }

        let matches = simplified_chinese_words_by_pinyin_prefix(self.prefix_text());
        let first_candidate = self.pinyin_candidate_page * PINYIN_CANDIDATES_PER_PAGE;
        for candidate_offset in 0..PINYIN_CANDIDATES_PER_PAGE {
            let label = matches
                .word_index_at(first_candidate + candidate_offset)
                .and_then(|index| word_for_index_in(self.language, index))
                .map(String::from)
                .unwrap_or_default();
            labels.push(label);
        }

        labels
    }

    pub(crate) fn is_pinyin_candidate_picker_open(&self) -> bool {
        self.pinyin_candidate_picker_open
    }

    pub(crate) fn pinyin_candidate_page_label(&self) -> String {
        let page_count = self.pinyin_candidate_page_count();
        if page_count == 0 {
            return String::new();
        }

        alloc::format!("{} / {page_count}", self.pinyin_candidate_page + 1)
    }

    pub(crate) fn has_previous_pinyin_candidate_page(&self) -> bool {
        self.pinyin_candidate_page > 0
    }

    pub(crate) fn has_next_pinyin_candidate_page(&self) -> bool {
        self.pinyin_candidate_page + 1 < self.pinyin_candidate_page_count()
    }

    pub(crate) fn open_pinyin_candidate_picker(&mut self) -> bool {
        if !self.can_open_pinyin_candidate_picker() {
            return false;
        }

        self.pinyin_candidate_picker_open = true;
        self.commit_feedback = CommitFeedback::None;
        true
    }

    pub(crate) fn close_pinyin_candidate_picker(&mut self) {
        self.pinyin_candidate_picker_open = false;
    }

    pub(crate) fn change_pinyin_candidate_page(&mut self, direction: i32) {
        if !self.pinyin_candidate_picker_open {
            return;
        }

        let page_count = self.pinyin_candidate_page_count();
        if page_count == 0 {
            return;
        }

        if direction < 0 {
            self.pinyin_candidate_page = self.pinyin_candidate_page.saturating_sub(1);
        } else if direction > 0 {
            self.pinyin_candidate_page = (self.pinyin_candidate_page + 1).min(page_count - 1);
        }
    }

    pub(crate) fn select_pinyin_candidate(&mut self, candidate_offset: i32) -> bool {
        if !self.pinyin_candidate_picker_open {
            return false;
        }

        let Ok(candidate_offset) = usize::try_from(candidate_offset) else {
            return false;
        };
        if candidate_offset >= PINYIN_CANDIDATES_PER_PAGE {
            return false;
        }

        let candidate_position =
            self.pinyin_candidate_page * PINYIN_CANDIDATES_PER_PAGE + candidate_offset;
        let matches = simplified_chinese_words_by_pinyin_prefix(self.prefix_text());
        let Some(index) = matches.word_index_at(candidate_position) else {
            return false;
        };

        self.accept_word_index(index);
        self.is_complete()
    }

    fn prefix_len_limit(&self) -> usize {
        match self.language {
            MnemonicLanguage::English => MAX_ENGLISH_PREFIX_LEN,
            MnemonicLanguage::SimplifiedChinese => MAX_WORD_PREFIX_LEN,
        }
    }

    fn prefix_has_matches(&self, prefix: &str) -> bool {
        self.prefix_match_count_for(prefix) > 0
    }

    pub(crate) fn prefix_match_count(&self) -> usize {
        self.prefix_match_count_for(self.prefix_text())
    }

    fn prefix_match_count_for(&self, prefix: &str) -> usize {
        match self.language {
            MnemonicLanguage::English => words_by_prefix(prefix).len(),
            MnemonicLanguage::SimplifiedChinese => {
                simplified_chinese_words_by_pinyin_prefix(prefix).len()
            }
        }
    }

    fn can_open_pinyin_candidate_picker(&self) -> bool {
        self.language == MnemonicLanguage::SimplifiedChinese
            && !self.prefix_text().is_empty()
            && self.selected_word_index().is_none()
            && self.prefix_match_count() > 0
    }

    fn pinyin_candidate_page_count(&self) -> usize {
        let candidate_count = self.prefix_match_count();
        if candidate_count == 0 {
            0
        } else {
            candidate_count.div_ceil(PINYIN_CANDIDATES_PER_PAGE)
        }
    }

    fn accept_word_index(&mut self, index: u16) {
        self.word_indices[self.word_count] = index;
        self.word_count += 1;
        self.prefix = [0; MAX_WORD_PREFIX_LEN];
        self.prefix_len = 0;
        self.last_letter_was_rejected = false;
        self.commit_feedback = CommitFeedback::None;
        self.pinyin_candidate_page = 0;
        self.pinyin_candidate_picker_open = false;
        self.clear_entropy();
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
    fn loading_indices_clears_rejected_letter_feedback() {
        let mut flow = LastWordFlow::default();
        flow.push_letter(0); // a
        flow.push_letter(0); // aa is not a BIP39 word prefix
        assert!(flow.last_letter_was_rejected);

        let indices = core::array::from_fn(|position| position as u16);
        assert_eq!(flow.load_word_indices(indices), Ok(()));

        assert!(!flow.last_letter_was_rejected);
        assert!(!flow.message().contains("No BIP39 word"));
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

    #[test]
    fn pinyin_picker_commits_the_selected_chinese_word_index() {
        let mut flow = LastWordFlow::default();
        flow.select_language(1);
        for byte in b"xing" {
            flow.push_letter(i32::from(*byte - b'a'));
        }

        assert!(flow.open_pinyin_candidate_picker());
        let matches = simplified_chinese_words_by_pinyin_prefix("xing");
        let candidate_position = (0..matches.len())
            .find(|position| matches.word_index_at(*position) == Some(56))
            .expect("pinyin alias must include BIP39 index 56");
        flow.pinyin_candidate_page = candidate_position / PINYIN_CANDIDATES_PER_PAGE;
        assert!(
            flow.pinyin_candidate_labels()
                .iter()
                .any(|label| label == "\u{884c}")
        );

        assert!(
            !flow.select_pinyin_candidate(
                i32::try_from(candidate_position % PINYIN_CANDIDATES_PER_PAGE)
                    .expect("candidate offset fits in i32")
            )
        );
        assert_eq!(flow.word_indices[0], 56);
        assert_eq!(flow.word_count, 1);
        assert_eq!(flow.prefix_text(), "");
        assert!(!flow.is_pinyin_candidate_picker_open());
    }

    #[test]
    fn language_choice_hides_until_reset() {
        let mut flow = LastWordFlow::default();
        assert!(flow.is_language_selection_visible());

        flow.select_language(0);
        assert!(!flow.is_language_selection_visible());

        flow.reset();
        assert!(flow.is_language_selection_visible());
        assert_eq!(flow.language_index(), 0);

        flow.push_letter(0);
        assert!(!flow.is_language_selection_visible());
    }
}

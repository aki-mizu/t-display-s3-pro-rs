use alloc::string::String;
use bip39_last_word::{MnemonicLanguage, PREFIX_WORD_COUNT};

mod entropy;
mod entry;
mod presentation;

const MAX_ENGLISH_PREFIX_LEN: usize = 4;
const MAX_PINYIN_PREFIX_LEN: usize = 6;
const MAX_WORD_PREFIX_LEN: usize = MAX_PINYIN_PREFIX_LEN;
const PINYIN_CANDIDATES_PER_PAGE: usize = 6;
const MAX_PASSPHRASE_BYTES: usize = 128;

/// Visible error result of an unsuccessful `Use word` action.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CommitFeedback {
    #[default]
    None,
    EnterLetters,
    MoreMatches {
        count: usize,
    },
}

/// Ephemeral touchscreen state for the BIP39 final-word helper.
///
/// The completed prefix remains only as word-list indices. It never builds or
/// stores a concatenated mnemonic phrase.
#[derive(Default)]
pub(crate) struct LastWordFlow {
    language: MnemonicLanguage,
    language_selected: bool,
    word_indices: [u16; PREFIX_WORD_COUNT],
    word_count: usize,
    prefix: [u8; MAX_WORD_PREFIX_LEN],
    prefix_len: usize,
    last_letter_was_rejected: bool,
    commit_feedback: CommitFeedback,
    pinyin_candidate_page: usize,
    pinyin_candidate_picker_open: bool,
    entropy_bits: u8,
    entropy_confirmed: bool,
    final_word_candidate_page: usize,
    final_word_picker_open: bool,
    passphrase: String,
    passphrase_draft: String,
    passphrase_editor_open: bool,
    passphrase_visible: bool,
}

impl LastWordFlow {
    fn clear_entropy(&mut self) {
        self.entropy_bits = 0;
        // Zero is a valid seven-bit entropy selection. Once all eleven
        // prefix words are present, show its final word and enable Address
        // immediately; incomplete and reset flows remain unselected.
        self.entropy_confirmed = self.is_complete();
        self.final_word_candidate_page = 0;
        self.final_word_picker_open = false;
    }

    pub(crate) fn finalized_entropy(&self) -> Option<[u8; 16]> {
        if !self.is_complete() || !self.entropy_confirmed {
            return None;
        }

        bip39_last_word::entropy_from_indices(&self.word_indices, self.entropy_bits).ok()
    }

    pub(crate) fn mnemonic_language(&self) -> MnemonicLanguage {
        self.language
    }

    pub(crate) fn open_passphrase_editor(&mut self) {
        self.passphrase_draft = self.passphrase.clone();
        self.passphrase_editor_open = true;
        self.passphrase_visible = false;
    }

    pub(crate) fn append_passphrase_key(&mut self, key: &str) {
        if !self.passphrase_editor_open
            || key.len() != 1
            || !key
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_graphic() || *byte == b' ')
            || self.passphrase_draft.len().saturating_add(key.len()) > MAX_PASSPHRASE_BYTES
        {
            return;
        }

        self.passphrase_draft.push_str(key);
    }

    pub(crate) fn backspace_passphrase(&mut self) {
        if self.passphrase_editor_open {
            self.passphrase_draft.pop();
        }
    }

    pub(crate) fn toggle_passphrase_visibility(&mut self) {
        if self.passphrase_editor_open {
            self.passphrase_visible = !self.passphrase_visible;
        }
    }

    pub(crate) fn save_passphrase(&mut self) -> bool {
        if !self.passphrase_editor_open {
            return false;
        }

        let changed = self.passphrase != self.passphrase_draft;
        self.passphrase = core::mem::take(&mut self.passphrase_draft);
        self.passphrase_editor_open = false;
        self.passphrase_visible = false;
        changed
    }

    pub(crate) fn cancel_passphrase_editor(&mut self) {
        self.passphrase_draft.clear();
        self.passphrase_editor_open = false;
        self.passphrase_visible = false;
    }

    pub(crate) fn is_passphrase_editor_open(&self) -> bool {
        self.passphrase_editor_open
    }

    pub(crate) fn is_passphrase_visible(&self) -> bool {
        self.passphrase_visible
    }

    pub(crate) fn passphrase_display(&self) -> String {
        if self.passphrase_visible {
            return self.passphrase_draft.clone();
        }

        let mut mask = String::with_capacity(self.passphrase_draft.len());
        for _ in self.passphrase_draft.bytes() {
            mask.push('*');
        }
        mask
    }

    pub(crate) fn has_passphrase(&self) -> bool {
        !self.passphrase.is_empty()
    }

    pub(crate) fn bip39_passphrase(&self) -> &str {
        &self.passphrase
    }

    pub(crate) fn clear_passphrase(&mut self) {
        self.passphrase.clear();
        self.passphrase_draft.clear();
        self.passphrase_editor_open = false;
        self.passphrase_visible = false;
    }
}

#[cfg(test)]
fn enter_word(flow: &mut LastWordFlow, word: &str) {
    for byte in word.bytes().take(MAX_ENGLISH_PREFIX_LEN) {
        flow.push_letter(i32::from(byte - b'a'));
    }
    assert!(flow.selected_word_index().is_some());
    flow.commit_word();
}

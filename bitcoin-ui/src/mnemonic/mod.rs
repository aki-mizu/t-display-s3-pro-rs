use bip39_last_word::{MnemonicLanguage, PREFIX_WORD_COUNT};

mod entropy;
mod entry;
mod presentation;

const MAX_ENGLISH_PREFIX_LEN: usize = 4;
const MAX_PINYIN_PREFIX_LEN: usize = 6;
const MAX_WORD_PREFIX_LEN: usize = MAX_PINYIN_PREFIX_LEN;
const PINYIN_CANDIDATES_PER_PAGE: usize = 6;

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
}

impl LastWordFlow {
    fn clear_entropy(&mut self) {
        self.entropy_bits = 0;
        self.entropy_confirmed = false;
        self.final_word_candidate_page = 0;
        self.final_word_picker_open = false;
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

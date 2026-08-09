use alloc::string::String;
use bip39_last_word::{
    Error as Bip39Error, PREFIX_WORD_COUNT, word_for_entropy_bits, word_for_index, word_index,
    words_by_prefix,
};

const MAX_WORD_PREFIX_LEN: usize = 4;
const WORD_LIST_HORIZONTAL_MARGIN_PX: usize = 1;
const WORD_LIST_DISPLAY_WIDTH_PX: usize = 480 - (WORD_LIST_HORIZONTAL_MARGIN_PX * 2);
const WORD_LIST_FONT_METRIC_SIZE_PX: usize = 9;
const WORD_LIST_GLYPH_ADVANCE_SCALE: usize = 64;
const WORD_LIST_MIN_FONT_SIZE_HUNDREDTHS: i32 = 600;
const WORD_LIST_MAX_FONT_SIZE_HUNDREDTHS: i32 = 2_000;

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
    word_indices: [u16; PREFIX_WORD_COUNT],
    word_count: usize,
    prefix: [u8; MAX_WORD_PREFIX_LEN],
    prefix_len: usize,
    last_letter_was_rejected: bool,
    commit_feedback: CommitFeedback,
    entropy_bits: u8,
    entropy_confirmed: bool,
}

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

    pub(crate) fn toggle_entropy_bit(&mut self, bit: i32) {
        if !self.is_complete() {
            return;
        }

        let Ok(bit) = u32::try_from(bit) else {
            return;
        };
        if bit >= 7 {
            return;
        }

        self.entropy_bits ^= 1_u8 << bit;
        self.entropy_confirmed = false;
    }

    pub(crate) fn set_entropy_die_face(&mut self, die: i32, face: i32) {
        if !self.is_complete() {
            return;
        }

        let Ok(face) = u8::try_from(face) else {
            return;
        };

        self.entropy_bits = match die {
            0 if (1..=8).contains(&face) => (self.entropy_bits & 0x0f) | ((face - 1) << 4),
            1 if (1..=16).contains(&face) => (self.entropy_bits & 0x70) | (face - 1),
            _ => return,
        };
        self.entropy_confirmed = false;
    }

    pub(crate) fn confirm_entropy(&mut self) {
        if self.is_complete() {
            // This explicit action is required even when the selected value is
            // 0000000. Eleven words alone do not select a unique final word.
            self.entropy_confirmed = true;
        }
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

    pub(crate) fn entry_text(&self) -> String {
        let prefix = self.prefix_text();
        if prefix.is_empty() {
            return String::from("Tap letters to filter the English word list.");
        }

        let matches = words_by_prefix(prefix);
        if let Some(index) = word_index(prefix) {
            return word_for_index(index)
                .map(|word| alloc::format!("Use exact: {word}"))
                .unwrap_or_default();
        }
        if matches.len() == 1 {
            return alloc::format!("Only match: {}", matches[0]);
        }

        alloc::format!("{} matches — add a letter.", matches.len())
    }

    pub(crate) fn message(&self) -> String {
        if self.last_letter_was_rejected {
            return String::from(
                "No BIP39 word can continue with that letter. Try another letter.",
            );
        }

        match self.commit_feedback {
            CommitFeedback::None => {}
            CommitFeedback::EnterLetters => {
                return String::from("Enter a BIP39 word before tapping Use word.");
            }
            CommitFeedback::MoreMatches { count } => {
                return alloc::format!(
                    "Use word needs one BIP39 word — {count} matching words remain."
                );
            }
        }

        if self.is_complete() {
            return String::from("All 11 words captured. Select the seven entropy bits.");
        }

        String::new()
    }

    pub(crate) fn entropy_bit_is_set(&self, bit: u8) -> bool {
        self.entropy_bits & (1_u8 << bit) != 0
    }

    pub(crate) fn entropy_octal_value(&self) -> u8 {
        self.entropy_bits >> 4
    }

    pub(crate) fn entropy_octal_face(&self) -> u8 {
        self.entropy_octal_value() + 1
    }

    pub(crate) fn entropy_hex_value(&self) -> u8 {
        self.entropy_bits & 0x0f
    }

    pub(crate) fn entropy_hex_face(&self) -> u8 {
        self.entropy_hex_value() + 1
    }

    pub(crate) fn entropy_octal_bits(&self) -> String {
        alloc::format!("{:03b}", self.entropy_octal_value())
    }

    pub(crate) fn entropy_hex_bits(&self) -> String {
        alloc::format!("{:04b}", self.entropy_hex_value())
    }

    /// Returns confirmed words separated by spaces for the input-progress row.
    pub(crate) fn confirmed_words_text(&self) -> String {
        if self.word_count == 0 {
            return String::new();
        }
        let mut text = String::new();
        for i in 0..self.word_count {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(word_for_index(self.word_indices[i]).unwrap_or("?"));
        }
        text
    }

    pub(crate) fn confirmed_words_font_size_hundredths(&self) -> i32 {
        let width_units = self
            .confirmed_words_text()
            .bytes()
            .map(word_list_glyph_advance_units)
            .sum::<usize>();
        if width_units == 0 {
            return WORD_LIST_MAX_FONT_SIZE_HUNDREDTHS;
        }

        let font_size = WORD_LIST_DISPLAY_WIDTH_PX
            * WORD_LIST_FONT_METRIC_SIZE_PX
            * WORD_LIST_GLYPH_ADVANCE_SCALE
            * 100
            / width_units;
        i32::try_from(font_size)
            .unwrap_or(WORD_LIST_MIN_FONT_SIZE_HUNDREDTHS)
            .clamp(
                WORD_LIST_MIN_FONT_SIZE_HUNDREDTHS,
                WORD_LIST_MAX_FONT_SIZE_HUNDREDTHS,
            )
    }

    pub(crate) fn candidate_word(&self) -> Option<&'static str> {
        self.entropy_confirmed
            .then(|| word_for_entropy_bits(&self.word_indices, self.entropy_bits).ok())
            .flatten()
    }

    fn clear_entropy(&mut self) {
        self.entropy_bits = 0;
        self.entropy_confirmed = false;
    }
}

fn word_list_glyph_advance_units(character: u8) -> usize {
    match character {
        b' ' | b'f' | b't' => 160,
        b'i' | b'j' | b'l' => 127,
        b'm' => 479,
        b'w' => 415,
        b'k' | b's' | b'v' | b'x' | b'y' | b'z' => 288,
        b'r' => 191,
        _ => 320,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enter_word(flow: &mut LastWordFlow, word: &str) {
        for byte in word.bytes().take(MAX_WORD_PREFIX_LEN) {
            flow.push_letter(i32::from(byte - b'a'));
        }
        assert!(flow.selected_word_index().is_some());
        flow.commit_word();
    }

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
    fn keeps_confirmed_words_visible_after_commit() {
        let mut flow = LastWordFlow::default();

        enter_word(&mut flow, "act");

        assert_eq!(flow.confirmed_words_text(), "act");
        assert_eq!(flow.commit_feedback, CommitFeedback::None);
    }

    #[test]
    fn shrinks_confirmed_words_to_fit_the_full_prefix() {
        let mut flow = LastWordFlow::default();

        enter_word(&mut flow, "act");
        assert_eq!(
            flow.confirmed_words_font_size_hundredths(),
            WORD_LIST_MAX_FONT_SIZE_HUNDREDTHS
        );

        for _ in 0..(PREFIX_WORD_COUNT - 1) {
            enter_word(&mut flow, "abandon");
        }
        assert_eq!(flow.confirmed_words_font_size_hundredths(), 1_110);
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
    fn use_word_adds_a_selected_word_to_progress() {
        let mut flow = LastWordFlow::default();
        for byte in b"act" {
            flow.push_letter(i32::from(*byte - b'a'));
        }

        assert!(!flow.commit_word());
        assert_eq!(flow.confirmed_words_text(), "act");
    }

    #[test]
    fn requires_explicit_entropy_confirmation_for_zero() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            enter_word(&mut flow, "abandon");
        }

        assert!(flow.is_complete());
        assert_eq!(flow.candidate_word(), None);
        flow.confirm_entropy();
        assert_eq!(flow.candidate_word(), Some("about"));
    }

    #[test]
    fn changing_entropy_hides_the_previous_result() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            enter_word(&mut flow, "abandon");
        }
        flow.confirm_entropy();
        assert_eq!(flow.candidate_word(), Some("about"));

        flow.toggle_entropy_bit(0);
        assert_eq!(flow.candidate_word(), None);
    }

    #[test]
    fn dice_faces_update_the_shared_entropy_bits() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            enter_word(&mut flow, "abandon");
        }

        flow.set_entropy_die_face(0, 6);
        flow.set_entropy_die_face(1, 13);

        assert_eq!(flow.entropy_bits, 0b101_1100);
        assert_eq!(flow.entropy_octal_value(), 5);
        assert_eq!(flow.entropy_hex_value(), 12);
        assert_eq!(flow.entropy_octal_face(), 6);
        assert_eq!(flow.entropy_hex_face(), 13);
        assert_eq!(flow.entropy_octal_bits(), "101");
        assert_eq!(flow.entropy_hex_bits(), "1100");

        flow.confirm_entropy();
        assert!(flow.candidate_word().is_some());

        flow.set_entropy_die_face(1, 14);
        assert_eq!(flow.candidate_word(), None);

        let entropy_bits = flow.entropy_bits;
        flow.set_entropy_die_face(0, 0);
        flow.set_entropy_die_face(1, 17);
        assert_eq!(flow.entropy_bits, entropy_bits);

        flow.set_entropy_die_face(0, 8);
        flow.set_entropy_die_face(1, 16);
        assert_eq!(flow.entropy_bits, 0b111_1111);
    }

    #[test]
    fn the_eleventh_confirmation_completes_the_prefix() {
        let mut flow = LastWordFlow::default();
        for _ in 0..(PREFIX_WORD_COUNT - 1) {
            enter_word(&mut flow, "abandon");
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

    #[test]
    fn selects_the_official_all_one_entropy_vector() {
        let words = [
            "legal", "winner", "thank", "year", "wave", "sausage", "worth", "useful", "legal",
            "winner", "thank",
        ];
        let mut flow = LastWordFlow::default();
        for word in words {
            enter_word(&mut flow, word);
        }
        for bit in 0..7 {
            flow.toggle_entropy_bit(bit);
        }

        flow.confirm_entropy();
        assert_eq!(flow.candidate_word(), Some("yellow"));
    }
}

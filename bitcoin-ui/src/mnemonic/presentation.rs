use alloc::string::String;
use bip39_last_word::{word_for_index, word_index, words_by_prefix};

use super::{CommitFeedback, LastWordFlow};

const WORD_LIST_HORIZONTAL_MARGIN_PX: usize = 1;
const WORD_LIST_DISPLAY_WIDTH_PX: usize = 480 - (WORD_LIST_HORIZONTAL_MARGIN_PX * 2);
const WORD_LIST_FONT_METRIC_SIZE_PX: usize = 9;
const WORD_LIST_GLYPH_ADVANCE_SCALE: usize = 64;
const WORD_LIST_MIN_FONT_SIZE_HUNDREDTHS: i32 = 600;
const WORD_LIST_MAX_FONT_SIZE_HUNDREDTHS: i32 = 2_000;

impl LastWordFlow {
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
    use bip39_last_word::PREFIX_WORD_COUNT;

    #[test]
    fn keeps_confirmed_words_visible_after_commit() {
        let mut flow = LastWordFlow::default();

        super::super::enter_word(&mut flow, "act");

        assert_eq!(flow.confirmed_words_text(), "act");
        assert_eq!(flow.commit_feedback, CommitFeedback::None);
    }

    #[test]
    fn shrinks_confirmed_words_to_fit_the_full_prefix() {
        let mut flow = LastWordFlow::default();

        super::super::enter_word(&mut flow, "act");
        assert_eq!(
            flow.confirmed_words_font_size_hundredths(),
            WORD_LIST_MAX_FONT_SIZE_HUNDREDTHS
        );

        for _ in 0..(PREFIX_WORD_COUNT - 1) {
            super::super::enter_word(&mut flow, "abandon");
        }
        assert_eq!(flow.confirmed_words_font_size_hundredths(), 1_110);
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
}

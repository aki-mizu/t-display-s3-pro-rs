use alloc::string::String;
use bip39_last_word::{MnemonicLanguage, word_for_index_in, word_index, words_by_prefix};

use super::{CommitFeedback, LastWordFlow};

const WORD_LIST_DISPLAY_WIDTH_PX: usize = 448;
const WORD_LIST_FONT_METRIC_SIZE_PX: usize = 9;
const WORD_LIST_GLYPH_ADVANCE_SCALE: usize = 64;
const WORD_LIST_MIN_FONT_SIZE_HUNDREDTHS: i32 = 600;
const WORD_LIST_MAX_FONT_SIZE_HUNDREDTHS: i32 = 2_000;

impl LastWordFlow {
    pub(crate) fn entry_text(&self) -> String {
        let prefix = self.prefix_text();
        if prefix.is_empty() {
            return match self.language {
                MnemonicLanguage::English => {
                    String::from("Tap letters to filter the English word list.")
                }
                MnemonicLanguage::SimplifiedChinese => {
                    String::from("Type pinyin to find a Simplified Chinese BIP39 word.")
                }
            };
        }

        match self.language {
            MnemonicLanguage::English => {
                let matches = words_by_prefix(prefix);
                if let Some(index) = word_index(prefix) {
                    return word_for_index_in(self.language, index)
                        .map(|word| alloc::format!("Use exact: {word}"))
                        .unwrap_or_default();
                }
                if matches.len() == 1 {
                    return alloc::format!("Only match: {}", matches[0]);
                }

                alloc::format!("{} matches - add a letter.", matches.len())
            }
            MnemonicLanguage::SimplifiedChinese => {
                if let Some(index) = self.selected_word_index() {
                    return word_for_index_in(self.language, index)
                        .map(|word| alloc::format!("Only match: {word}"))
                        .unwrap_or_default();
                }

                alloc::format!(
                    "{} Chinese matches - tap Use word to choose.",
                    self.prefix_match_count()
                )
            }
        }
    }

    pub(crate) fn message(&self) -> String {
        if self.last_letter_was_rejected {
            return match self.language {
                MnemonicLanguage::English => {
                    String::from("No BIP39 word can continue with that letter. Try another letter.")
                }
                MnemonicLanguage::SimplifiedChinese => {
                    String::from("No BIP39 word matches that pinyin. Try another letter.")
                }
            };
        }

        match self.commit_feedback {
            CommitFeedback::None => {}
            CommitFeedback::EnterLetters => {
                return match self.language {
                    MnemonicLanguage::English => {
                        String::from("Enter a BIP39 word before tapping Use word.")
                    }
                    MnemonicLanguage::SimplifiedChinese => {
                        String::from("Enter pinyin before tapping Use word.")
                    }
                };
            }
            CommitFeedback::MoreMatches { count } => {
                return match self.language {
                    MnemonicLanguage::English => alloc::format!(
                        "Use word needs one BIP39 word - {count} matching words remain."
                    ),
                    MnemonicLanguage::SimplifiedChinese => {
                        alloc::format!("Choose one of {count} Chinese BIP39 words.")
                    }
                };
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
            text.push_str(word_for_index_in(self.language, self.word_indices[i]).unwrap_or("?"));
        }
        text
    }

    pub(crate) fn confirmed_words_font_size_hundredths(&self) -> i32 {
        let width_units = self
            .confirmed_words_text()
            .chars()
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

fn word_list_glyph_advance_units(character: char) -> usize {
    match character {
        ' ' | 'f' | 't' => 160,
        'i' | 'j' | 'l' => 127,
        'm' => 479,
        'w' => 415,
        'k' | 's' | 'v' | 'x' | 'y' | 'z' => 288,
        'r' => 191,
        _ if character.is_ascii() => 320,
        _ => 640,
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
        assert_eq!(flow.confirmed_words_font_size_hundredths(), 1_040);
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
    fn renders_confirmed_words_in_the_selected_language() {
        let mut flow = LastWordFlow::default();
        flow.select_language(1);
        flow.word_indices[0] = 0;
        flow.word_count = 1;

        assert_eq!(flow.confirmed_words_text(), "\u{7684}");
    }
}

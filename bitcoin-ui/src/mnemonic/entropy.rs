use alloc::{string::String, vec::Vec};
use bip39_last_word::{CANDIDATE_COUNT, word_for_entropy_bits_in};

use super::LastWordFlow;

const FINAL_WORD_CANDIDATES_PER_PAGE: usize = 12;

impl LastWordFlow {
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
        self.entropy_confirmed = true;
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
        self.entropy_confirmed = true;
    }

    pub(crate) fn open_final_word_picker(&mut self) -> bool {
        if !self.is_complete() {
            return false;
        }

        self.final_word_candidate_page =
            usize::from(self.entropy_bits) / FINAL_WORD_CANDIDATES_PER_PAGE;
        self.final_word_picker_open = true;
        true
    }

    pub(crate) fn close_final_word_picker(&mut self) {
        self.final_word_picker_open = false;
    }

    pub(crate) fn is_final_word_picker_open(&self) -> bool {
        self.final_word_picker_open
    }

    pub(crate) fn final_word_candidate_labels(&self) -> Vec<String> {
        let mut labels = Vec::with_capacity(FINAL_WORD_CANDIDATES_PER_PAGE);
        if !self.final_word_picker_open || !self.is_complete() {
            labels.resize(FINAL_WORD_CANDIDATES_PER_PAGE, String::new());
            return labels;
        }

        let first_candidate = self.final_word_candidate_page * FINAL_WORD_CANDIDATES_PER_PAGE;
        for candidate_offset in 0..FINAL_WORD_CANDIDATES_PER_PAGE {
            let candidate_position = first_candidate + candidate_offset;
            let label = (candidate_position < CANDIDATE_COUNT)
                .then(|| u8::try_from(candidate_position).ok())
                .flatten()
                .and_then(|entropy_bits| {
                    word_for_entropy_bits_in(self.language, &self.word_indices, entropy_bits).ok()
                })
                .map(String::from)
                .unwrap_or_default();
            labels.push(label);
        }

        labels
    }

    pub(crate) fn final_word_candidate_page_label(&self) -> String {
        alloc::format!(
            "{} / {}",
            self.final_word_candidate_page + 1,
            self.final_word_candidate_page_count()
        )
    }

    pub(crate) fn has_previous_final_word_candidate_page(&self) -> bool {
        self.final_word_candidate_page > 0
    }

    pub(crate) fn has_next_final_word_candidate_page(&self) -> bool {
        self.final_word_candidate_page + 1 < self.final_word_candidate_page_count()
    }

    pub(crate) fn change_final_word_candidate_page(&mut self, direction: i32) {
        if !self.final_word_picker_open {
            return;
        }

        if direction < 0 {
            self.final_word_candidate_page = self.final_word_candidate_page.saturating_sub(1);
        } else if direction > 0 {
            self.final_word_candidate_page = (self.final_word_candidate_page + 1)
                .min(self.final_word_candidate_page_count() - 1);
        }
    }

    pub(crate) fn select_final_word_candidate(&mut self, candidate_offset: i32) -> bool {
        if !self.final_word_picker_open {
            return false;
        }

        let Ok(candidate_offset) = usize::try_from(candidate_offset) else {
            return false;
        };
        if candidate_offset >= FINAL_WORD_CANDIDATES_PER_PAGE {
            return false;
        }

        let candidate_position =
            self.final_word_candidate_page * FINAL_WORD_CANDIDATES_PER_PAGE + candidate_offset;
        if candidate_position >= CANDIDATE_COUNT {
            return false;
        }

        self.entropy_bits =
            u8::try_from(candidate_position).expect("candidate position fits in u8");
        self.entropy_confirmed = true;
        self.final_word_picker_open = false;
        true
    }

    pub(crate) fn is_entropy_confirmed(&self) -> bool {
        self.entropy_confirmed
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

    pub(crate) fn entropy_octal_bits(&self) -> alloc::string::String {
        alloc::format!("{:03b}", self.entropy_octal_value())
    }

    pub(crate) fn entropy_hex_bits(&self) -> alloc::string::String {
        alloc::format!("{:04b}", self.entropy_hex_value())
    }

    pub(crate) fn candidate_word(&self) -> Option<&'static str> {
        self.entropy_confirmed
            .then(|| {
                word_for_entropy_bits_in(self.language, &self.word_indices, self.entropy_bits).ok()
            })
            .flatten()
    }

    fn final_word_candidate_page_count(&self) -> usize {
        CANDIDATE_COUNT.div_ceil(FINAL_WORD_CANDIDATES_PER_PAGE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bip39_last_word::PREFIX_WORD_COUNT;

    #[test]
    fn default_zero_entropy_reveals_the_final_word() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            super::super::enter_word(&mut flow, "abandon");
        }

        assert!(flow.is_complete());
        assert!(flow.is_entropy_confirmed());
        assert_eq!(flow.candidate_word(), Some("about"));
        flow.set_entropy_die_face(0, 1);
        assert_eq!(flow.candidate_word(), Some("about"));
    }

    #[test]
    fn changing_entropy_reveals_the_updated_word() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            super::super::enter_word(&mut flow, "abandon");
        }
        flow.set_entropy_die_face(0, 1);
        assert_eq!(flow.candidate_word(), Some("about"));

        flow.toggle_entropy_bit(0);
        assert_eq!(
            flow.candidate_word(),
            word_for_entropy_bits_in(flow.language, &flow.word_indices, flow.entropy_bits).ok()
        );
    }

    #[test]
    fn dice_faces_update_the_shared_entropy_bits() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            super::super::enter_word(&mut flow, "abandon");
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

        assert!(flow.candidate_word().is_some());

        flow.set_entropy_die_face(1, 14);
        assert!(flow.candidate_word().is_some());

        let entropy_bits = flow.entropy_bits;
        flow.set_entropy_die_face(0, 0);
        flow.set_entropy_die_face(1, 17);
        assert_eq!(flow.entropy_bits, entropy_bits);

        flow.set_entropy_die_face(0, 8);
        flow.set_entropy_die_face(1, 16);
        assert_eq!(flow.entropy_bits, 0b111_1111);
    }

    #[test]
    fn selects_the_official_all_one_entropy_vector() {
        let words = [
            "legal", "winner", "thank", "year", "wave", "sausage", "worth", "useful", "legal",
            "winner", "thank",
        ];
        let mut flow = LastWordFlow::default();
        for word in words {
            super::super::enter_word(&mut flow, word);
        }
        for bit in 0..7 {
            flow.toggle_entropy_bit(bit);
        }

        assert_eq!(flow.candidate_word(), Some("yellow"));
    }

    #[test]
    fn commits_or_cancels_a_masked_passphrase() {
        let mut flow = LastWordFlow::default();

        flow.open_passphrase_editor();
        for key in ["T", "R", "E", "Z", "O", "R"] {
            flow.append_passphrase_key(key);
        }
        assert_eq!(flow.passphrase_display(), "******");
        assert!(!flow.has_passphrase());

        flow.cancel_passphrase_editor();
        assert!(!flow.has_passphrase());
        assert!(!flow.is_passphrase_editor_open());

        flow.open_passphrase_editor();
        for key in ["T", "R", "E", "Z", "O", "R"] {
            flow.append_passphrase_key(key);
        }
        flow.toggle_passphrase_visibility();
        assert_eq!(flow.passphrase_display(), "TREZOR");
        assert!(flow.save_passphrase());
        assert_eq!(flow.bip39_passphrase(), "TREZOR");
        assert!(flow.has_passphrase());
        assert!(!flow.is_passphrase_editor_open());
    }

    #[test]
    fn accepts_every_printable_ascii_passphrase_character() {
        let mut flow = LastWordFlow::default();
        let mut expected = String::new();

        flow.open_passphrase_editor();
        for byte in 0x20_u8..=0x7e {
            let bytes = [byte];
            let key = core::str::from_utf8(&bytes).expect("printable ASCII");
            flow.append_passphrase_key(key);
            expected.push(char::from(byte));
        }

        flow.toggle_passphrase_visibility();
        assert_eq!(expected.len(), 95);
        assert_eq!(flow.passphrase_display(), expected);
    }

    #[test]
    fn selecting_a_final_word_candidate_confirms_its_entropy_bits() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            super::super::enter_word(&mut flow, "abandon");
        }

        assert!(flow.open_final_word_picker());
        assert_eq!(flow.final_word_candidate_page_label(), "1 / 11");
        assert_eq!(flow.final_word_candidate_labels()[0], "about");

        flow.change_final_word_candidate_page(1);
        assert_eq!(flow.final_word_candidate_page_label(), "2 / 11");
        assert!(flow.select_final_word_candidate(0));

        assert_eq!(flow.entropy_bits, 12);
        assert!(flow.is_entropy_confirmed());
        assert!(!flow.is_final_word_picker_open());
        assert_eq!(
            flow.candidate_word(),
            word_for_entropy_bits_in(flow.language, &flow.word_indices, 12).ok()
        );
    }
}

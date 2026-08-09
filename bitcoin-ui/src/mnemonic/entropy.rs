use bip39_last_word::word_for_entropy_bits;

use super::LastWordFlow;

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
            .then(|| word_for_entropy_bits(&self.word_indices, self.entropy_bits).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bip39_last_word::PREFIX_WORD_COUNT;

    #[test]
    fn requires_explicit_entropy_confirmation_for_zero() {
        let mut flow = LastWordFlow::default();
        for _ in 0..PREFIX_WORD_COUNT {
            super::super::enter_word(&mut flow, "abandon");
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
            super::super::enter_word(&mut flow, "abandon");
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

        flow.confirm_entropy();
        assert_eq!(flow.candidate_word(), Some("yellow"));
    }
}

use alloc::rc::Rc;
use core::cell::RefCell;
use slint::{ComponentHandle, VecModel};

use crate::{generated::AppWindow, mnemonic::LastWordFlow};

pub(crate) fn configure_mnemonic_flow(window: &AppWindow) -> Rc<RefCell<LastWordFlow>> {
    let flow = Rc::new(RefCell::new(LastWordFlow::default()));
    sync_mnemonic_view(window, &flow.borrow());

    let weak_window = window.as_weak();
    window.on_mnemonic_key({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move |key| {
            flow.borrow_mut().push_letter(key);
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_mnemonic_backspace({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            flow.borrow_mut().backspace();
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_mnemonic_commit({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            // A completed prefix always leads to the final-word screen. This
            // also gives a visible recovery path if a repaint was interrupted
            // immediately after the eleventh confirmation.
            let complete = {
                let mut flow = flow.borrow_mut();
                if flow.is_complete() {
                    true
                } else {
                    flow.commit_word()
                }
            };
            if let Some(window) = weak_window.upgrade() {
                if complete {
                    window.set_current_screen(1);
                }
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_mnemonic_reset({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            flow.borrow_mut().reset();
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_entropy_toggle({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move |bit| {
            flow.borrow_mut().toggle_entropy_bit(bit);
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_entropy_select_die({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move |die, face| {
            flow.borrow_mut().set_entropy_die_face(die, face);
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    window.on_entropy_confirm({
        let flow = Rc::clone(&flow);
        let weak_window = weak_window.clone();
        move || {
            flow.borrow_mut().confirm_entropy();
            if let Some(window) = weak_window.upgrade() {
                sync_mnemonic_view(&window, &flow.borrow());
            }
        }
    });

    flow
}

pub(crate) fn sync_mnemonic_view(window: &AppWindow, flow: &LastWordFlow) {
    window.set_mnemonic_prefix(flow.prefix_text().into());
    window.set_mnemonic_entry(flow.entry_text().into());
    window.set_mnemonic_message(flow.message().into());
    window.set_mnemonic_confirmed_words(flow.confirmed_words_text().into());
    window.set_mnemonic_confirmed_words_font_size_hundredths(
        flow.confirmed_words_font_size_hundredths(),
    );
    window.set_mnemonic_can_commit(flow.selected_word_index().is_some());
    window.set_mnemonic_next_letters(VecModel::from_slice(&flow.next_letter_enabled()));
    window.set_entropy_octal_face(i32::from(flow.entropy_octal_face()));
    window.set_entropy_hex_face(i32::from(flow.entropy_hex_face()));
    window.set_entropy_octal_bits(flow.entropy_octal_bits().into());
    window.set_entropy_hex_bits(flow.entropy_hex_bits().into());
    window.set_entropy_bit_64(flow.entropy_bit_is_set(6));
    window.set_entropy_bit_32(flow.entropy_bit_is_set(5));
    window.set_entropy_bit_16(flow.entropy_bit_is_set(4));
    window.set_entropy_bit_8(flow.entropy_bit_is_set(3));
    window.set_entropy_bit_4(flow.entropy_bit_is_set(2));
    window.set_entropy_bit_2(flow.entropy_bit_is_set(1));
    window.set_entropy_bit_1(flow.entropy_bit_is_set(0));
    window.set_candidate_word(flow.candidate_word().unwrap_or("").into());
    window.set_entropy_confirmed(flow.is_entropy_confirmed());
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{boxed::Box, rc::Rc};
    use slint::{
        Model, PhysicalSize,
        platform::{
            Platform, PlatformError, WindowAdapter,
            software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
        },
    };

    struct TestPlatform(Rc<MinimalSoftwareWindow>);

    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.0.clone())
        }

        fn duration_since_start(&self) -> core::time::Duration {
            core::time::Duration::ZERO
        }
    }

    #[test]
    fn slint_key_callback_updates_the_prefix() {
        let renderer_window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(TestPlatform(renderer_window.clone()))).ok();
        renderer_window.set_size(PhysicalSize::new(480, 222));
        let window = AppWindow::new().expect("create test window");
        let flow = configure_mnemonic_flow(&window);

        window.invoke_mnemonic_key(0);

        assert_eq!(window.get_mnemonic_prefix().as_str(), "a");
        let next_letters = window.get_mnemonic_next_letters();
        assert_eq!(next_letters.row_data(usize::from(b'b' - b'a')), Some(true));
        assert_eq!(next_letters.row_data(usize::from(b'a' - b'a')), Some(false));

        let word_indices = core::array::from_fn(|position| position as u16);
        flow.borrow_mut()
            .load_word_indices(word_indices)
            .expect("valid test word indices");
        sync_mnemonic_view(&window, &flow.borrow());
        assert!(!window.get_entropy_confirmed());

        window.invoke_entropy_confirm();
        assert!(window.get_entropy_confirmed());

        window.invoke_entropy_toggle(0);
        assert!(!window.get_entropy_confirmed());
    }
}

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::event::Input;

/// Stateful translator from crossterm key events to high-level Inputs.
/// Holds the digit buffer used by `<N>g` gotos and the one-shot `g` state
/// that turns a second `g` into "go to page 1".
#[derive(Debug, Default)]
pub struct InputMapper {
    digits: String,
    g_pending: bool,
}

impl InputMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending_digits(&self) -> &str {
        &self.digits
    }

    pub fn is_awaiting_g(&self) -> bool {
        self.g_pending
    }

    pub fn reset(&mut self) {
        self.digits.clear();
        self.g_pending = false;
    }

    pub fn map(&mut self, ev: KeyEvent) -> Input {
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        let shift = ev.modifiers.contains(KeyModifiers::SHIFT);

        if ctrl
            && matches!(
                ev.code,
                KeyCode::Char('c') | KeyCode::Char('C')
            )
        {
            self.reset();
            return Input::Quit;
        }

        match ev.code {
            KeyCode::Esc => {
                self.reset();
                Input::Quit
            }
            KeyCode::Char('q') => {
                self.reset();
                Input::Quit
            }
            KeyCode::Char('n') | KeyCode::Char('j') | KeyCode::Char(' ') => {
                self.reset();
                Input::Next
            }
            KeyCode::Char('p') | KeyCode::Char('k') => {
                self.reset();
                Input::Prev
            }
            KeyCode::Backspace => {
                self.reset();
                Input::Prev
            }
            KeyCode::Char('G') => {
                self.reset();
                Input::Last
            }
            KeyCode::Char('g') if !shift => {
                if let Some(page) = parse_digits(&self.digits) {
                    self.reset();
                    Input::Goto(page)
                } else if self.g_pending {
                    self.reset();
                    Input::First
                } else {
                    self.g_pending = true;
                    Input::GotoStart
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                // Digits buffer `<N>g`; pressing one after `g<pending>`
                // cancels the `gg` intent cleanly.
                self.g_pending = false;
                self.digits.push(c);
                Input::Digit(c)
            }
            _ => Input::Noop,
        }
    }
}

fn parse_digits(s: &str) -> Option<u32> {
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn next_on_nj_space() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::Char('n'))), Input::Next);
        assert_eq!(m.map(key(KeyCode::Char('j'))), Input::Next);
        assert_eq!(m.map(key(KeyCode::Char(' '))), Input::Next);
    }

    #[test]
    fn prev_on_pk_backspace() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::Char('p'))), Input::Prev);
        assert_eq!(m.map(key(KeyCode::Char('k'))), Input::Prev);
        assert_eq!(m.map(key(KeyCode::Backspace)), Input::Prev);
    }

    #[test]
    fn quit_on_q_esc_ctrl_c() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::Char('q'))), Input::Quit);
        assert_eq!(m.map(key(KeyCode::Esc)), Input::Quit);
        assert_eq!(
            m.map(key_mod(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Input::Quit
        );
    }

    #[test]
    fn capital_g_jumps_to_last() {
        let mut m = InputMapper::new();
        assert_eq!(
            m.map(key_mod(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Input::Last
        );
    }

    #[test]
    fn double_g_jumps_to_first() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::Char('g'))), Input::GotoStart);
        assert!(m.is_awaiting_g());
        assert_eq!(m.map(key(KeyCode::Char('g'))), Input::First);
        assert!(!m.is_awaiting_g());
    }

    #[test]
    fn digit_then_g_emits_goto() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::Char('1'))), Input::Digit('1'));
        assert_eq!(m.map(key(KeyCode::Char('2'))), Input::Digit('2'));
        assert_eq!(m.pending_digits(), "12");
        assert_eq!(m.map(key(KeyCode::Char('g'))), Input::Goto(12));
        assert!(m.pending_digits().is_empty());
    }

    #[test]
    fn digit_cancels_g_pending() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::Char('g'))), Input::GotoStart);
        assert_eq!(m.map(key(KeyCode::Char('5'))), Input::Digit('5'));
        assert!(!m.is_awaiting_g());
        assert_eq!(m.map(key(KeyCode::Char('g'))), Input::Goto(5));
    }

    #[test]
    fn navigation_key_resets_digit_buffer() {
        let mut m = InputMapper::new();
        m.map(key(KeyCode::Char('1')));
        m.map(key(KeyCode::Char('0')));
        assert_eq!(m.pending_digits(), "10");
        assert_eq!(m.map(key(KeyCode::Char('n'))), Input::Next);
        assert!(m.pending_digits().is_empty());
    }

    #[test]
    fn unknown_keys_are_noop() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::F(1))), Input::Noop);
        assert_eq!(m.map(key(KeyCode::Char('z'))), Input::Noop);
    }

    #[test]
    fn bare_g_without_digits_starts_gg_sequence() {
        let mut m = InputMapper::new();
        assert_eq!(m.map(key(KeyCode::Char('g'))), Input::GotoStart);
        assert!(m.is_awaiting_g());
        assert_eq!(m.pending_digits(), "");
    }
}

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode,
    MouseButton as CrosstermMouseButton, MouseEvent as CrosstermMouseEvent,
    MouseEventKind as CrosstermMouseEventKind,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ModifierKey {
    Shift,
    Control,
    Alt,
    Super,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[non_exhaustive]
pub enum Key {
    Character(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    BackTab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Function(u8),
    Modifier(ModifierKey),
    Null,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum KeyKind {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct KeyStroke {
    pub key: Key,
    pub modifiers: Modifiers,
    pub kind: KeyKind,
}

impl KeyStroke {
    /// Returns text input while excluding releases and command modifiers.
    pub fn text_character(&self) -> Option<char> {
        match self {
            Self {
                key: Key::Character(character),
                modifiers:
                    Modifiers {
                        control: false,
                        alt: false,
                        super_key: false,
                        ..
                    },
                kind: KeyKind::Press | KeyKind::Repeat,
            } => Some(*character),
            _ => None,
        }
    }

    pub fn pty_bytes(&self, application_cursor: bool) -> Option<Vec<u8>> {
        if self.modifiers.super_key {
            return None;
        }
        if let Key::Character(character) = self.key {
            if self.modifiers.control && character.is_ascii() {
                let byte = (character.to_ascii_uppercase() as u8) & 0x1f;
                return Some(if self.modifiers.alt {
                    vec![0x1b, byte]
                } else {
                    vec![byte]
                });
            }
            let mut bytes = Vec::new();
            if self.modifiers.alt {
                bytes.push(0x1b);
            }
            let mut encoded = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            return Some(bytes);
        }
        let modifier = 1
            + u8::from(self.modifiers.shift)
            + 2 * u8::from(self.modifiers.alt)
            + 4 * u8::from(self.modifiers.control);
        let sequence = match self.key {
            Key::Enter => "\r".to_owned(),
            Key::Escape => "\x1b".to_owned(),
            Key::Backspace => "\x7f".to_owned(),
            Key::Tab if self.modifiers.shift => "\x1b[Z".to_owned(),
            Key::Tab | Key::BackTab => "\t".to_owned(),
            Key::Up if modifier == 1 && application_cursor => "\x1bOA".to_owned(),
            Key::Down if modifier == 1 && application_cursor => "\x1bOB".to_owned(),
            Key::Right if modifier == 1 && application_cursor => "\x1bOC".to_owned(),
            Key::Left if modifier == 1 && application_cursor => "\x1bOD".to_owned(),
            Key::Up if modifier == 1 => "\x1b[A".to_owned(),
            Key::Down if modifier == 1 => "\x1b[B".to_owned(),
            Key::Right if modifier == 1 => "\x1b[C".to_owned(),
            Key::Left if modifier == 1 => "\x1b[D".to_owned(),
            Key::Up => format!("\x1b[1;{modifier}A"),
            Key::Down => format!("\x1b[1;{modifier}B"),
            Key::Right => format!("\x1b[1;{modifier}C"),
            Key::Left => format!("\x1b[1;{modifier}D"),
            Key::Home if modifier == 1 => "\x1b[H".to_owned(),
            Key::End if modifier == 1 => "\x1b[F".to_owned(),
            Key::Home => format!("\x1b[1;{modifier}H"),
            Key::End => format!("\x1b[1;{modifier}F"),
            Key::Delete if modifier == 1 => "\x1b[3~".to_owned(),
            Key::Delete => format!("\x1b[3;{modifier}~"),
            _ => return None,
        };
        Some(sequence.into_bytes())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum MouseEventKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[non_exhaustive]
pub enum TerminalEvent {
    Key(KeyStroke),
    Resize { columns: u16, rows: u16 },
    Paste(String),
    FocusGained,
    FocusLost,
    Mouse(MouseEvent),
}

impl From<KeyModifiers> for Modifiers {
    fn from(value: KeyModifiers) -> Self {
        Self {
            shift: value.contains(KeyModifiers::SHIFT),
            control: value.contains(KeyModifiers::CONTROL),
            alt: value.contains(KeyModifiers::ALT),
            super_key: value.contains(KeyModifiers::SUPER),
        }
    }
}

impl From<KeyEventKind> for KeyKind {
    fn from(value: KeyEventKind) -> Self {
        match value {
            KeyEventKind::Press => Self::Press,
            KeyEventKind::Repeat => Self::Repeat,
            KeyEventKind::Release => Self::Release,
        }
    }
}

impl From<KeyCode> for Key {
    fn from(value: KeyCode) -> Self {
        match value {
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Enter => Self::Enter,
            KeyCode::Left => Self::Left,
            KeyCode::Right => Self::Right,
            KeyCode::Up => Self::Up,
            KeyCode::Down => Self::Down,
            KeyCode::Home => Self::Home,
            KeyCode::End => Self::End,
            KeyCode::PageUp => Self::PageUp,
            KeyCode::PageDown => Self::PageDown,
            KeyCode::Tab => Self::Tab,
            KeyCode::BackTab => Self::BackTab,
            KeyCode::Delete => Self::Delete,
            KeyCode::Insert => Self::Insert,
            KeyCode::F(number) => Self::Function(number),
            KeyCode::Modifier(modifier) => Self::Modifier(modifier.into()),
            KeyCode::Char(character) => Self::Character(character),
            KeyCode::Esc => Self::Escape,
            _ => Self::Null,
        }
    }
}

impl From<ModifierKeyCode> for ModifierKey {
    fn from(value: ModifierKeyCode) -> Self {
        match value {
            ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => Self::Shift,
            ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => Self::Control,
            ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => Self::Alt,
            ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => Self::Super,
            _ => Self::Other,
        }
    }
}

impl From<KeyEvent> for KeyStroke {
    fn from(value: KeyEvent) -> Self {
        Self {
            key: value.code.into(),
            modifiers: value.modifiers.into(),
            kind: value.kind.into(),
        }
    }
}

impl From<CrosstermMouseButton> for MouseButton {
    fn from(value: CrosstermMouseButton) -> Self {
        match value {
            CrosstermMouseButton::Left => Self::Left,
            CrosstermMouseButton::Right => Self::Right,
            CrosstermMouseButton::Middle => Self::Middle,
        }
    }
}

impl From<CrosstermMouseEventKind> for MouseEventKind {
    fn from(value: CrosstermMouseEventKind) -> Self {
        match value {
            CrosstermMouseEventKind::Down(button) => Self::Down(button.into()),
            CrosstermMouseEventKind::Up(button) => Self::Up(button.into()),
            CrosstermMouseEventKind::Drag(button) => Self::Drag(button.into()),
            CrosstermMouseEventKind::Moved => Self::Moved,
            CrosstermMouseEventKind::ScrollUp => Self::ScrollUp,
            CrosstermMouseEventKind::ScrollDown => Self::ScrollDown,
            CrosstermMouseEventKind::ScrollLeft => Self::ScrollLeft,
            CrosstermMouseEventKind::ScrollRight => Self::ScrollRight,
        }
    }
}

impl From<CrosstermMouseEvent> for MouseEvent {
    fn from(value: CrosstermMouseEvent) -> Self {
        Self {
            kind: value.kind.into(),
            column: value.column,
            row: value.row,
            modifiers: value.modifiers.into(),
        }
    }
}

impl TryFrom<Event> for TerminalEvent {
    type Error = ();

    fn try_from(value: Event) -> Result<Self, Self::Error> {
        match value {
            Event::Key(key) => Ok(Self::Key(key.into())),
            Event::Resize(columns, rows) => Ok(Self::Resize { columns, rows }),
            Event::Paste(text) => Ok(Self::Paste(text)),
            Event::FocusGained => Ok(Self::FocusGained),
            Event::FocusLost => Ok(Self::FocusLost),
            Event::Mouse(event) => Ok(Self::Mouse(event.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode,
        MouseButton as CrosstermMouseButton, MouseEvent as CrosstermMouseEvent,
        MouseEventKind as CrosstermMouseEventKind,
    };

    use super::{Key, KeyKind, KeyStroke, ModifierKey, Modifiers, TerminalEvent};

    fn key(event: KeyEvent) -> super::KeyStroke {
        let TerminalEvent::Key(stroke) = TerminalEvent::try_from(Event::Key(event)).unwrap() else {
            panic!("expected key event");
        };
        stroke
    }

    #[test]
    fn legacy_escape_alt_and_function_keys_normalize_deterministically() {
        let escape = key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(escape.key, Key::Escape);
        assert_eq!(escape.kind, KeyKind::Press);

        let alt = key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT));
        assert_eq!(alt.key, Key::Character('x'));
        assert!(alt.modifiers.alt);

        let function = key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
        assert_eq!(function.key, Key::Function(12));
        assert_eq!(function.kind, KeyKind::Press);
    }

    #[test]
    fn enhanced_repeat_release_and_disambiguated_keys_remain_distinct() {
        let repeat = key(KeyEvent::new_with_kind(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));
        let release = key(KeyEvent::new_with_kind(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert_eq!(repeat.kind, KeyKind::Repeat);
        assert_eq!(release.kind, KeyKind::Release);

        let tab = key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let control_i = key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL));
        assert_eq!(tab.key, Key::Tab);
        assert_eq!(control_i.key, Key::Character('i'));
        assert!(control_i.modifiers.control);
        assert_ne!(tab, control_i);
        assert_eq!(repeat.text_character(), Some('j'));
        assert_eq!(release.text_character(), None);
        assert_eq!(control_i.text_character(), None);
    }

    #[test]
    fn enhanced_modifier_keys_preserve_press_and_release_identity() {
        let press = key(KeyEvent::new_with_kind(
            KeyCode::Modifier(ModifierKeyCode::LeftAlt),
            KeyModifiers::ALT,
            KeyEventKind::Press,
        ));
        let release = key(KeyEvent::new_with_kind(
            KeyCode::Modifier(ModifierKeyCode::LeftAlt),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert_eq!(press.key, Key::Modifier(ModifierKey::Alt));
        assert_eq!(press.kind, KeyKind::Press);
        assert_eq!(release.key, Key::Modifier(ModifierKey::Alt));
        assert_eq!(release.kind, KeyKind::Release);
    }

    #[test]
    fn normalized_events_round_trip_as_public_evidence_records() {
        let event = super::TerminalEvent::Key(super::KeyStroke {
            key: super::Key::PageDown,
            modifiers: super::Modifiers {
                shift: true,
                control: false,
                alt: true,
                super_key: false,
            },
            kind: super::KeyKind::Release,
        });
        let encoded = serde_json::to_vec(&event).unwrap();
        let decoded = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn normalized_keys_encode_native_pty_line_editor_input() {
        let plain = KeyStroke {
            key: Key::Character('é'),
            modifiers: Modifiers::default(),
            kind: KeyKind::Press,
        };
        assert_eq!(plain.pty_bytes(false).unwrap(), "é".as_bytes());
        let control_r = KeyStroke {
            key: Key::Character('r'),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
            kind: KeyKind::Press,
        };
        assert_eq!(control_r.pty_bytes(false).unwrap(), [0x12]);
        let alt_left = KeyStroke {
            key: Key::Left,
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            kind: KeyKind::Press,
        };
        assert_eq!(alt_left.pty_bytes(false).unwrap(), b"\x1b[1;3D");
    }

    #[test]
    fn mouse_coordinates_buttons_modifiers_and_wheel_are_preserved() {
        let event = TerminalEvent::try_from(Event::Mouse(CrosstermMouseEvent {
            kind: CrosstermMouseEventKind::Drag(CrosstermMouseButton::Left),
            column: 17,
            row: 9,
            modifiers: KeyModifiers::SHIFT,
        }))
        .unwrap();
        assert_eq!(
            event,
            TerminalEvent::Mouse(super::MouseEvent {
                kind: super::MouseEventKind::Drag(super::MouseButton::Left),
                column: 17,
                row: 9,
                modifiers: super::Modifiers {
                    shift: true,
                    ..super::Modifiers::default()
                },
            })
        );

        let wheel = TerminalEvent::try_from(Event::Mouse(CrosstermMouseEvent {
            kind: CrosstermMouseEventKind::ScrollDown,
            column: 2,
            row: 3,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
        assert!(matches!(
            wheel,
            TerminalEvent::Mouse(super::MouseEvent {
                kind: super::MouseEventKind::ScrollDown,
                ..
            })
        ));
    }
}

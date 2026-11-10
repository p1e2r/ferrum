// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Converts winit keyboard events into the W3C key events Servo expects.
//!
//! winit's `KeyCode` and `NamedKey` variants are named after the W3C UI Events
//! `code`/`key` values, so most map by name; the few that differ are listed here.

use std::str::FromStr;

use keyboard_types::{Code, Key, KeyState, Location, Modifiers, NamedKey};
use servo::KeyboardEvent;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key as WKey, KeyCode, KeyLocation, ModifiersState, NamedKey as WNamed, PhysicalKey};

fn key(event: &KeyEvent) -> Key {
    match &event.logical_key {
        WKey::Character(s) => Key::Character(s.to_string()),
        WKey::Named(WNamed::Space) => Key::Character(" ".into()),
        WKey::Named(named) => {
            let name = match named {
                WNamed::Super => "Meta".to_string(),
                other => format!("{other:?}"),
            };
            NamedKey::from_str(&name).map(Key::Named).unwrap_or(Key::Named(NamedKey::Unidentified))
        },
        WKey::Unidentified(_) | WKey::Dead(_) => match &event.text {
            Some(text) if !text.is_empty() => Key::Character(text.to_string()),
            _ if matches!(event.logical_key, WKey::Dead(_)) => Key::Named(NamedKey::Dead),
            _ => Key::Named(NamedKey::Unidentified),
        },
    }
}

fn code(event: &KeyEvent) -> Code {
    let PhysicalKey::Code(code) = event.physical_key else {
        return Code::Unidentified;
    };
    let name = match code {
        KeyCode::SuperLeft => "MetaLeft".to_string(),
        KeyCode::SuperRight => "MetaRight".to_string(),
        other => format!("{other:?}"),
    };
    Code::from_str(&name).unwrap_or(Code::Unidentified)
}

fn location(event: &KeyEvent) -> Location {
    match event.location {
        KeyLocation::Standard => Location::Standard,
        KeyLocation::Left => Location::Left,
        KeyLocation::Right => Location::Right,
        KeyLocation::Numpad => Location::Numpad,
    }
}

pub fn modifiers(state: ModifiersState) -> Modifiers {
    let mut m = Modifiers::empty();
    m.set(Modifiers::CONTROL, state.control_key());
    m.set(Modifiers::SHIFT, state.shift_key());
    m.set(Modifiers::ALT, state.alt_key());
    m.set(Modifiers::META, state.super_key());
    m
}

pub fn keyboard_event(event: &KeyEvent, state: ModifiersState) -> KeyboardEvent {
    let key_state = match event.state {
        ElementState::Pressed => KeyState::Down,
        ElementState::Released => KeyState::Up,
    };
    KeyboardEvent::new_without_event(
        key_state,
        key(event),
        code(event),
        location(event),
        modifiers(state),
        event.repeat,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_line_up() {
        assert_eq!(Code::from_str(&format!("{:?}", KeyCode::KeyA)).unwrap(), Code::KeyA);
        assert_eq!(Code::from_str(&format!("{:?}", KeyCode::ArrowLeft)).unwrap(), Code::ArrowLeft);
        assert_eq!(Code::from_str(&format!("{:?}", KeyCode::NumpadEnter)).unwrap(), Code::NumpadEnter);
        assert_eq!(NamedKey::from_str(&format!("{:?}", WNamed::Backspace)).unwrap(), NamedKey::Backspace);
        assert_eq!(NamedKey::from_str(&format!("{:?}", WNamed::Enter)).unwrap(), NamedKey::Enter);
        assert_eq!(NamedKey::from_str(&format!("{:?}", WNamed::PageDown)).unwrap(), NamedKey::PageDown);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
use super::{Check, Samples, TestResult};
use lavashell_core::geometry::Point;
use lavashell_core::input_method::{
    Capitalization, FieldText, TextAction, TextCommand, TextPurpose,
};
use lavashell_core::keyboard::{Key, Shortcut};
use lavashell_core::ui::{InputModifiers, PointerKind, TouchId};
use lavashell_core::units::DpExt;
use lavashell_core::view::{Cursor, InputEvent, TouchSample};

fn touch(x: f64, kind: PointerKind) -> TouchSample {
    TouchSample {
        id: TouchId { seat: 2, point: -3 },
        at: Point::new(x.dp(), 9.dp()),
        kind,
    }
}

fn inputs() -> Samples<InputEvent> {
    let modifiers = InputModifiers {
        control: true,
        shift: false,
        alt: true,
    };
    let shortcut = Shortcut {
        key: Key::Character('z'),
        control: true,
        shift: false,
        alt: true,
    };
    every_variant! {
        InputEvent::Leave => InputEvent::Leave, [0];
        InputEvent::CancelPointer => InputEvent::CancelPointer, [1];
        InputEvent::WindowFocus(_) => InputEvent::WindowFocus(true), [2, 1];
        InputEvent::KeyRelease(_) => InputEvent::KeyRelease(Key::Escape), [3, 5];
        InputEvent::Modifiers(_) => InputEvent::Modifiers(modifiers), [4, 1, 0, 1];
        InputEvent::Context(_) => InputEvent::Context(Point::new(1.dp(), 2.dp())), [
            5, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 64,
        ];
        InputEvent::Move(_) => InputEvent::Move(Point::new(3.dp(), 4.dp())), [
            6, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 16, 64,
        ];
        InputEvent::Press(_) => InputEvent::Press(Point::new(5.dp(), 6.dp())), [
            7, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 24, 64,
        ];
        InputEvent::Release(_) => InputEvent::Release(Point::new(7.dp(), 8.dp())), [
            8, 0, 0, 0, 0, 0, 0, 28, 64, 0, 0, 0, 0, 0, 0, 32, 64,
        ];
        InputEvent::TouchDown(_) => InputEvent::TouchDown(touch(10.0, PointerKind::Finger)), [
            9, 2, 5, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 34, 64, 1,
        ];
        InputEvent::TouchMove(_) => InputEvent::TouchMove(touch(11.0, PointerKind::Pen)), [
            10, 2, 5, 0, 0, 0, 0, 0, 0, 38, 64, 0, 0, 0, 0, 0, 0, 34, 64, 2,
        ];
        InputEvent::TouchUp(_) => InputEvent::TouchUp(touch(12.0, PointerKind::Mouse)), [
            11, 2, 5, 0, 0, 0, 0, 0, 0, 40, 64, 0, 0, 0, 0, 0, 0, 34, 64, 0,
        ];
        InputEvent::TouchCancel(_) => InputEvent::TouchCancel(TouchId { seat: 4, point: 5 }),
            [12, 4, 10];
        InputEvent::Scroll { .. } => InputEvent::Scroll {
            at: Point::new(13.dp(), 14.dp()),
            delta: Point::new(0.dp(), (-24).dp()),
        }, [
            13, 0, 0, 0, 0, 0, 0, 42, 64, 0, 0, 0, 0, 0, 0, 44, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 56, 192,
        ];
        InputEvent::Text(_) => InputEvent::Text("é".into()), [14, 2, 195, 169];
        InputEvent::Key { .. } => InputEvent::Key { key: Key::Tab, shift: true }, [15, 0, 1];
        InputEvent::Shortcut(_) => InputEvent::Shortcut(shortcut), [16, 14, 1, 122, 1, 0, 1];
    }
}

fn keys() -> Samples<Key> {
    every_variant! {
        Key::Tab => Key::Tab, [0]; Key::Enter => Key::Enter, [1]; Key::Space => Key::Space, [2];
        Key::Backspace => Key::Backspace, [3]; Key::Delete => Key::Delete, [4];
        Key::Escape => Key::Escape, [5]; Key::Left => Key::Left, [6]; Key::Right => Key::Right, [7];
        Key::Up => Key::Up, [8]; Key::Down => Key::Down, [9]; Key::PageUp => Key::PageUp, [10];
        Key::PageDown => Key::PageDown, [11]; Key::Home => Key::Home, [12];
        Key::End => Key::End, [13]; Key::Character(_) => Key::Character('ß'), [14, 2, 195, 159];
    }
}

fn pointer_kinds() -> Samples<PointerKind> {
    every_variant! {
        PointerKind::Mouse => PointerKind::Mouse, [0];
        PointerKind::Finger => PointerKind::Finger, [1]; PointerKind::Pen => PointerKind::Pen, [2];
    }
}

fn text_commands() -> Samples<TextCommand> {
    every_variant! {
        TextCommand::Commit(_) => TextCommand::Commit("ok".into()), [0, 2, 111, 107];
        TextCommand::DeleteBackward => TextCommand::DeleteBackward, [1];
        TextCommand::Key(_) => TextCommand::Key(Key::Enter), [2, 1];
        TextCommand::Action(_) => TextCommand::Action(TextAction::Send), [3, 4];
    }
}

fn text_actions() -> Samples<TextAction> {
    every_variant! {
        TextAction::Search => TextAction::Search, [0]; TextAction::Done => TextAction::Done, [1];
        TextAction::Next => TextAction::Next, [2]; TextAction::Go => TextAction::Go, [3];
        TextAction::Send => TextAction::Send, [4];
    }
}

fn text_purposes() -> Samples<TextPurpose> {
    every_variant! {
        TextPurpose::Text => TextPurpose::Text, [0];
        TextPurpose::Number => TextPurpose::Number, [1];
        TextPurpose::Phone => TextPurpose::Phone, [2];
        TextPurpose::Email => TextPurpose::Email, [3]; TextPurpose::Url => TextPurpose::Url, [4];
    }
}

fn capitalizations() -> Samples<Capitalization> {
    every_variant! {
        Capitalization::None => Capitalization::None, [0];
        Capitalization::Sentences => Capitalization::Sentences, [1];
        Capitalization::Words => Capitalization::Words, [2];
        Capitalization::Characters => Capitalization::Characters, [3];
    }
}

fn field_texts() -> Samples<FieldText> {
    every_variant! {
        FieldText::Protected => FieldText::Protected, [0];
        FieldText::Unknown => FieldText::Unknown, [1];
        FieldText::BeforeCursor(_) => FieldText::BeforeCursor("Lis".into()), [2, 3, 76, 105, 115];
    }
}

fn cursors() -> Samples<Cursor> {
    every_variant! {
        Cursor::Default => Cursor::Default, [0]; Cursor::Pointer => Cursor::Pointer, [1];
        Cursor::Text => Cursor::Text, [2];
    }
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&inputs())?;
    check.samples(&keys())?;
    check.samples(&pointer_kinds())?;
    check.samples(&text_commands())?;
    check.samples(&text_actions())?;
    check.samples(&text_purposes())?;
    check.samples(&capitalizations())?;
    check.samples(&field_texts())?;
    check.samples(&cursors())
}

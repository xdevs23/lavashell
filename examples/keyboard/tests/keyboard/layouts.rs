// SPDX-License-Identifier: GPL-3.0-only
use super::{Play, platform, query, settled, shot, showing, touch_key};
use icu_locale_core::langid;
use lavashell::keyboard::{ENGLISH, FRENCH, GERMAN, Layout, PORTUGUESE};
use lavashell::prelude::*;
use lavashell_example_support::{Start, node};
use lavashell_on_screen_keyboard::{
    CopyKey, Error, copy,
    model::Demo,
    stage::{QUERY, finger, key_bounds, tap, tap_at, touch},
};
use std::{cell::Cell, rc::Rc};

fn tap_globe(view: &mut View, _: Viewport, _: &Demo) -> Result<(), Error> {
    tap(view, copy(CopyKey::Globe))
}

fn shows(layout: &'static Layout) -> impl FnOnce(&View, Viewport, &Demo) -> Result<(), Error> {
    move |view, _, demo| {
        assert_eq!(demo.layout.get().id(), layout.id());
        let space = node(view, &Role::Button, copy(CopyKey::Space))?;
        assert_eq!(
            space.value,
            Some(AccessibleValue::Text(layout.name().to_string()))
        );
        Ok(())
    }
}

fn layouts_menu(shown: bool) -> impl FnOnce(&View, Viewport, &Demo) -> Result<(), Error> {
    move |view, _, _| {
        let menu = node(view, &Role::Menu, copy(CopyKey::KeyboardLayouts));
        assert_eq!(menu.is_ok(), shown);
        Ok(())
    }
}

pub(super) fn layouts(play: &mut Play) {
    for (layout, name) in [
        (&GERMAN, "deutsch"),
        (&PORTUGUESE, "portugues"),
        (&ENGLISH, "english"),
    ] {
        play.push(settled(), tap_globe);
        play.check(settled(), shows(layout));
        shot(play, settled(), &format!("layout-{name}"));
    }
    play.push(settled(), |view, _, demo| {
        (demo.settings).update(|settings| settings.layouts.push(&FRENCH));
        platform(view, |facts| facts.locales.push(langid!("fr")));
        demo.layout.set(&FRENCH);
        Ok(())
    });
    play.check(settled(), shows(&FRENCH));
    shot(play, settled(), "layout-francais");
    play.push(settled(), |view, _, demo| {
        (demo.settings)
            .update(|settings| settings.layouts.retain(|layout| layout.id() != FRENCH.id()));
        platform(view, |facts| {
            facts.locales.retain(|locale| *locale != langid!("fr"));
        });
        demo.layout.set(&ENGLISH);
        Ok(())
    });

    let english = Rc::new(Cell::new(Dp::ZERO));
    let measured = Rc::clone(&english);
    play.push(settled(), move |view, viewport, demo| {
        measured.set(key_bounds(view, "q")?.width);
        tap_globe(view, viewport, demo)
    });
    play.check(settled(), move |view, _, _| {
        assert!(
            key_bounds(view, "q")?.width < english.get(),
            "German has eleven keys in a row"
        );
        Ok(())
    });
    play.push(settled(), |view, _, _| {
        touch_key(view, "Switch layout", InputEvent::TouchDown)
    });
    play.push(showing(CopyKey::KeyboardLayouts), |view, _, _| {
        touch_key(view, "Switch layout", InputEvent::TouchUp)
    });
    play.check(Start::NEXT, layouts_menu(true));
    shot(play, settled(), "globe-list");
    play.push(settled(), |view, _, _| {
        let portuguese = node(view, &Role::MenuItem, "Português")?.bounds.center();
        tap_at(view, portuguese);
        Ok(())
    });
    play.check(settled(), layouts_menu(false));
    play.check(Start::NEXT, shows(&PORTUGUESE));
    play.push(Start::NEXT, tap_globe);
    play.push(Start::NEXT, tap_globe);
    play.check(settled(), shows(&GERMAN));
}

pub(super) fn further_letters(play: &mut Play) {
    let more_letters = || showing(CopyKey::MoreLetters);
    play.push(settled(), |view, _, _| {
        touch_key(view, "a", InputEvent::TouchDown)
    });
    shot(play, more_letters(), "accents-deutsch-a");
    play.check(Start::NEXT, |view, _, _| {
        assert!(node(view, &Role::MenuItem, "á").is_ok());
        assert!(
            node(view, &Role::MenuItem, "ä").is_err(),
            "ä has a key on the German layout"
        );
        Ok(())
    });
    play.push(Start::NEXT, |view, _, _| {
        touch_key(view, "a", InputEvent::TouchUp)
    });
    play.check(Start::NEXT, query(format!("{QUERY}à")));
    play.push(Start::NEXT, |view, _, _| tap(view, copy(CopyKey::Delete)));
    play.push(Start::NEXT, tap_globe);
    play.push(settled(), |view, _, _| {
        touch_key(view, "c", InputEvent::TouchDown)
    });
    shot(play, more_letters(), "accents-portugues-c");
    play.push(Start::NEXT, |view, _, _| {
        view.input(InputEvent::TouchCancel(touch(1)));
        Ok(())
    });
    play.push(settled(), |view, _, _| {
        touch_key(view, "e", InputEvent::TouchDown)
    });
    shot(play, more_letters(), "accents-portugues-e");
    play.push(Start::NEXT, |view, _, _| {
        let circumflex = node(view, &Role::MenuItem, "ê")?.bounds.center();
        view.input(InputEvent::TouchMove(finger(1, circumflex)));
        view.input(InputEvent::TouchUp(finger(1, circumflex)));
        Ok(())
    });
    play.check(Start::NEXT, query(format!("{QUERY}ê")));
    play.push(Start::NEXT, |view, _, _| tap(view, copy(CopyKey::Delete)));
    play.check(Start::NEXT, query(QUERY.to_owned()));
    play.push(Start::NEXT, tap_globe);
    play.check(settled(), shows(&ENGLISH));
}

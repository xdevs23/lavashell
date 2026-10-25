// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::board::{Board, Key as Cap, Page};

const PAGES: [Page; 3] = [Page::Letters, Page::Symbols, Page::MoreSymbols];

const QWERTY: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];

fn static_layout(
    id: &'static str,
    rows: [&'static str; 3],
    further_letters: &'static [(char, &'static str)],
    currency: &'static str,
) -> Result<&'static Layout, TestError> {
    let layout = Layout::new(LayoutSpec {
        id,
        language: language!("xx"),
        name: id,
        arrangement: "QWERTY",
        rows,
        further_letters,
        currency,
    });
    Ok(Box::leak(Box::new(layout?)))
}

fn offered(host: &mut Host, name: &str) -> Result<Vec<String>, TestError> {
    let at = key(&host.view, name)?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    let mut items: Vec<_> = (host.view.accessibility().nodes.values())
        .filter(|node| node.role == Role::MenuItem)
        .collect();
    items.sort_by(|first, second| first.bounds.x.total_cmp(&second.bounds.x));
    let names = items.iter().filter_map(|item| item.name.clone()).collect();
    host.view.input(InputEvent::TouchCancel(touch(1)));
    host.frame()?;
    Ok(names)
}

#[test]
fn every_row_of_every_page_spans_the_columns_of_its_top_row() {
    for layout in LAYOUTS {
        for (page, layouts) in PAGES
            .into_iter()
            .flat_map(|page| [(page, vec![layout]), (page, vec![layout, &ENGLISH])])
        {
            let board = Board::new(layout, page, &layouts);
            let globe = layouts.len() > 1;
            for row in &board.rows {
                let spans: f64 = row.iter().map(|slot| slot.span).sum();
                let gap = (spans - board.columns).abs();
                assert!(
                    gap <= board.columns * f64::EPSILON,
                    "{} {page:?}",
                    layout.id()
                );
            }
            let keys: Vec<_> = board.keys().map(|(_, key)| key).collect();
            assert_eq!(keys.contains(&Cap::Globe), globe && page == Page::Letters);
        }
    }
}

#[test]
fn the_second_page_of_symbols_offers_the_currencies_left_off_the_first() -> Result<(), TestError> {
    let currencies = |currency: &'static str| -> Result<[bool; 4], TestError> {
        let layout = static_layout("xx-currency", QWERTY, &[], currency)?;
        let first = Board::new(layout, Page::Symbols, &[layout]);
        assert!(first.keys().any(|(_, key)| key == Cap::Currency));
        let second = Board::new(layout, Page::MoreSymbols, &[layout]);
        let keys: Vec<_> = second.keys().map(|(_, key)| key).collect();
        Ok(['€', '£', '$', '¥'].map(|symbol| keys.contains(&Cap::Char(symbol))))
    };
    assert_eq!(currencies("€")?, [false, true, true, true]);
    assert_eq!(currencies("$")?, [true, true, false, true]);
    assert_eq!(currencies("R$")?, [true, true, true, false]);
    Ok(())
}

#[test]
fn the_currency_key_types_the_currency_of_the_shown_layout() -> Result<(), TestError> {
    let real = static_layout("xx-real", QWERTY, &[], "R$")?;
    let rupee = static_layout("xx-rupee", QWERTY, &[], "₹")?;
    let (config, facts) = offering(vec![real, rupee]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    for name in ["Symbols", "R$", "Letters", "Switch layout", "Symbols", "₹"] {
        tap(&mut host.view, name)?;
        host.frame()?;
    }
    assert_eq!(host.sent(), [Host::commit("R$"), Host::commit("₹")]);
    Ok(())
}

#[test]
fn a_long_press_offers_the_letters_of_the_shown_layout_and_then_of_every_configured_layout()
-> Result<(), TestError> {
    let first = static_layout("xx-first", QWERTY, &[('a', "àá"), ('o', "ö")], "$")?;
    let second = static_layout("xx-second", QWERTY, &[('a', "âá"), ('c', "ç")], "$")?;
    let (config, facts) = offering(vec![first, second]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    assert_eq!(offered(&mut host, "a")?, ["à", "á", "â"]);
    assert_eq!(offered(&mut host, "c")?, ["ç"]);
    assert_eq!(offered(&mut host, "o")?, ["ö"]);
    host.settings
        .update(|settings| settings.layouts = vec![second, first]);
    host.frame()?;
    assert_eq!(offered(&mut host, "a")?, ["â", "á", "à"]);
    host.settings
        .update(|settings| settings.layouts = vec![first]);
    host.frame()?;
    assert!(offered(&mut host, "c")?.is_empty());
    assert!(host.sent().is_empty(), "a cancelled press types nothing");
    Ok(())
}

#[test]
fn a_long_press_leaves_out_the_letters_that_have_a_key_on_the_shown_layout() -> Result<(), TestError>
{
    let keyed = static_layout(
        "xx-keyed",
        ["qwertyuiop", "asdfghjklà", "zxcvbnm"],
        &[('a', "à")],
        "$",
    )?;
    let plain = static_layout("xx-plain", QWERTY, &[('a', "áà")], "$")?;
    let (config, facts) = offering(vec![keyed, plain]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    assert!(has(&host.view, &Role::Button, "à"));
    assert_eq!(offered(&mut host, "a")?, ["á"]);
    tap(&mut host.view, "Switch layout")?;
    host.frame()?;
    assert!(!has(&host.view, &Role::Button, "à"));
    assert_eq!(offered(&mut host, "a")?, ["á", "à"]);
    Ok(())
}

#[test]
fn a_long_press_offers_the_letters_of_the_shown_layout_while_no_layout_is_configured()
-> Result<(), TestError> {
    let shown = static_layout("xx-shown", QWERTY, &[('a', "àá")], "$")?;
    let (config, facts) = offering(vec![]);
    let mut host = Host::on(config, facts)?;
    host.layout.set(shown);
    host.start(TextHints::default(), "")?;
    assert_eq!(offered(&mut host, "a")?, ["à", "á"]);
    Ok(())
}

#[test]
fn german_has_umlaut_keys_in_eleven_columns_and_a_long_press_types_the_first_letter()
-> Result<(), TestError> {
    let (config, facts) = offering(vec![&GERMAN]);
    let mut host = Host::on(config, facts)?;
    host.start(TextHints::default(), "")?;
    for umlaut in ["ü", "ö", "ä"] {
        assert!(has(&host.view, &Role::Button, umlaut), "a key for {umlaut}");
    }
    let (a, q) = (key(&host.view, "a")?, key(&host.view, "q")?);
    assert!(
        (a.x - q.x).abs() < 0.01.dp(),
        "rows of eleven letters line up"
    );
    let bounds =
        |name| -> Result<Rect, TestError> { Ok(node(&host.view, &Role::Button, name)?.bounds) };
    let (first, last) = (bounds("q")?, bounds("ü")?);
    let gap = bounds("w")?.x - (first.x + first.width);
    let row = last.x + last.width - first.x;
    assert!(
        (first.width * 11.0 + gap * 10.0 - row).abs() < 0.01.dp(),
        "eleven keys and ten gaps fill the row"
    );
    let at = key(&host.view, "s")?;
    host.view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    }));
    host.at(host.time + LONG_PRESS)?;
    host.view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    }));
    assert_eq!(host.sent(), [Host::commit("ß")]);
    Ok(())
}

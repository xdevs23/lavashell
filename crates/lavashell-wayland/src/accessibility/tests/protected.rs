// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use accesskit_consumer::Tree;
use lavashell_core::prelude::{
    Density, Dp, DpExt, InputEvent, Key, Modifier, ProtectedText, Size, TextFieldOptions,
    TextMeasurer, TextStyle, View, Viewport,
};

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, value: &str, style: &TextStyle<Dp>, width: Dp) -> Size {
        let natural: Dp = value.chars().map(|_| style.size).sum();
        Size::new(natural.min(width), style.size)
    }
}

#[test]
fn native_password_role_has_no_credential_value_runs_or_selection_actions() -> Result<(), TestError>
{
    let secret = ProtectedText::new();
    let private = secret.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.protected_input(
            private.clone(),
            TextFieldOptions::default(),
            Modifier::new().size(200.dp(), 40.dp()),
        );
    });
    let viewport = Viewport::new(Size::new(220.dp(), 50.dp()), Density::ONE);
    assert!(!view.frame(viewport, &mut Metrics)?.changes.is_empty());
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Text("synthetic".into()));
    for revealed in [false, true] {
        secret.set_revealed(revealed);
        assert!(!view.frame(viewport, &mut Metrics)?.changes.is_empty());
        let native = mapping::tree(view.accessibility(), &mut ids::Ids::default(), "test");
        let field = native_node(&native, NativeRole::PasswordInput)?;
        assert_eq!(native.focus, field.0);
        assert_eq!(field.1.value(), Some(""));
        assert!(field.1.children().is_empty());
        assert!(field.1.text_selection().is_none());
        assert!(!field.1.supports_action(Action::SetValue));
        assert!(!field.1.supports_action(Action::ReplaceSelectedText));
        assert!(!field.1.supports_action(Action::SetTextSelection));
        assert!(
            native
                .nodes
                .iter()
                .all(|(_, node)| node.value().is_none_or(str::is_empty))
        );
        assert!(
            native
                .nodes
                .iter()
                .all(|(_, node)| node.role() != NativeRole::TextRun)
        );
        let id = field.0;
        let consumer = Tree::new(native, true);
        let field = (consumer.state())
            .node_by_tree_local_id(id, TreeId::ROOT)
            .ok_or(TestError::MissingNativeNode(id))?;
        assert_eq!(field.role(), NativeRole::PasswordInput);
        assert_eq!(field.value().as_deref(), Some(""));
        assert!(!field.supports_text_ranges());
        assert!(field.text_selection().is_none());
        assert!(field.children().next().is_none());
    }
    secret.take().expose(|value| assert_eq!(value, "synthetic"));
    Ok(())
}

// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::*;
use std::{collections::BTreeSet, rc::Rc};

/// The selected items of a list, stored by data key, with the anchor of a Shift range and the
/// current item. Because the selection uses data keys, recycling, sorting and filtering the list
/// leave it unchanged.
#[derive(Clone, Debug)]
pub struct Selection<K> {
    selected: Rc<BTreeSet<K>>,
    anchor: Option<K>,
    current: Option<K>,
}

impl<K> Default for Selection<K> {
    fn default() -> Self {
        Self {
            selected: Rc::new(BTreeSet::new()),
            anchor: None,
            current: None,
        }
    }
}

impl<K: Clone + Ord> Selection<K> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.selected.contains(key)
    }

    pub fn keys(&self) -> &BTreeSet<K> {
        &self.selected
    }

    pub fn current(&self) -> Option<&K> {
        self.current.as_ref()
    }

    pub fn clear(&mut self) {
        Rc::make_mut(&mut self.selected).clear();
        self.anchor = None;
        self.current = None;
    }

    pub fn select_all(&mut self, order: &[K]) {
        self.selected = Rc::new(order.iter().cloned().collect());
    }

    pub fn retain(&mut self, mut exists: impl FnMut(&K) -> bool) {
        Rc::make_mut(&mut self.selected).retain(&mut exists);
        if self.anchor.as_ref().is_some_and(|k| !exists(k)) {
            self.anchor = None;
        }
        if self.current.as_ref().is_some_and(|k| !exists(k)) {
            self.current = None;
        }
    }

    pub fn select(&mut self, key: K, modifiers: InputModifiers, order: &[K]) {
        if modifiers.shift {
            let a = self
                .anchor
                .as_ref()
                .and_then(|k| order.iter().position(|x| x == k));
            let b = order.iter().position(|x| x == &key);
            if !modifiers.control {
                Rc::make_mut(&mut self.selected).clear();
            }
            if let (Some(a), Some(b)) = (a, b) {
                Rc::make_mut(&mut self.selected).extend(order[a.min(b)..=a.max(b)].iter().cloned());
            } else {
                let _selected_either_way = Rc::make_mut(&mut self.selected).insert(key.clone());
                self.anchor = Some(key.clone());
            }
        } else {
            if modifiers.control {
                let selected = Rc::make_mut(&mut self.selected);
                if !selected.remove(&key) {
                    let _absent_after_the_remove = selected.insert(key.clone());
                }
            } else {
                let selected = Rc::make_mut(&mut self.selected);
                selected.clear();
                let _absent_after_the_clear = selected.insert(key.clone());
            }
            self.anchor = Some(key.clone());
        }
        self.current = Some(key);
    }

    pub fn navigate(
        &mut self,
        shortcut: Shortcut,
        order: &[K],
        columns: usize,
        page_rows: usize,
    ) -> Option<usize> {
        if order.is_empty() {
            return None;
        }
        let index = self
            .current
            .as_ref()
            .and_then(|k| order.iter().position(|x| x == k))
            .unwrap_or(0);
        let columns = columns.max(1);
        let next = match shortcut.key {
            Key::Up => index.saturating_sub(columns),
            Key::Down => (index + columns).min(order.len() - 1),
            Key::Left => index.saturating_sub(1),
            Key::Right => (index + 1).min(order.len() - 1),
            Key::Home => 0,
            Key::End => order.len() - 1,
            Key::PageUp => index.saturating_sub(columns.saturating_mul(page_rows.max(1))),
            Key::PageDown => index
                .saturating_add(columns.saturating_mul(page_rows.max(1)))
                .min(order.len() - 1),
            Key::Character('a') if shortcut.control => {
                self.select_all(order);
                return Some(index);
            }
            _ => return None,
        };
        if shortcut.control && !shortcut.shift {
            self.current = Some(order[next].clone());
        } else {
            self.select(
                order[next].clone(),
                InputModifiers {
                    shift: shortcut.shift,
                    control: shortcut.control,
                    alt: shortcut.alt,
                },
                order,
            );
        }
        Some(next)
    }
}

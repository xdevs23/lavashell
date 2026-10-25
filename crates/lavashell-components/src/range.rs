// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::*;
use std::{cell::RefCell, rc::Rc};

pub(super) trait RangeValue: Copy + 'static {
    fn from_number(number: f64) -> Self;

    fn number(self) -> f64;
}

impl RangeValue for f64 {
    fn from_number(number: f64) -> Self {
        number
    }

    fn number(self) -> f64 {
        self
    }
}

impl RangeValue for Dp {
    fn from_number(number: f64) -> Self {
        Dp::new(number)
    }

    fn number(self) -> f64 {
        self.get()
    }
}

/// A value from zero to a maximum, moved by pointer drags, keys and assistive technology. Every
/// input reads the value through one function and changes it through one callback.
#[derive(Clone)]
pub(super) struct RangeControl<V> {
    value: Rc<dyn Fn() -> f64>,
    maximum: NonNegative,
    step: Option<Positive>,
    page: Option<Positive>,
    axis: Axis,
    change: Rc<RefCell<dyn FnMut(V)>>,
}

impl<V: RangeValue> RangeControl<V> {
    /// A range from zero to `maximum`, read through `value` and changed through `change`. The
    /// arrow and page keys do not move the value until `stepped` sets their distances. A value
    /// read outside the range is clamped into the range.
    ///
    /// # Errors
    ///
    /// Fails when `maximum` is not finite or below zero.
    pub(super) fn new(
        value: impl Fn() -> V + 'static,
        maximum: V,
        axis: Axis,
        change: impl FnMut(V) + 'static,
    ) -> Result<Self, NumberError> {
        let maximum = NonNegative::try_new(maximum.number())?;
        Ok(Self {
            value: Rc::new(move || value().number().clamp(0.0, maximum.get())),
            maximum,
            step: None,
            page: None,
            axis,
            change: Rc::new(RefCell::new(change)),
        })
    }

    /// The control with `step` as the distance of one arrow key press and of the Increment and
    /// Decrement actions, and `page` as the distance of one Page Up or Page Down press. A distance
    /// of zero or `None` leaves the matching keys without effect.
    ///
    /// # Errors
    ///
    /// Fails when a distance is not finite or below zero.
    pub(super) fn stepped(self, step: Option<V>, page: Option<V>) -> Result<Self, NumberError> {
        let distance = |distance: Option<V>| {
            (distance.map(V::number))
                .filter(|&distance| distance != 0.0)
                .map(Positive::try_new)
                .transpose()
        };
        Ok(Self {
            step: distance(step)?,
            page: distance(page)?,
            ..self
        })
    }

    pub(super) fn value(&self) -> V {
        V::from_number((self.value)())
    }

    pub(super) fn set(&self, value: V) {
        self.set_number(value.number());
    }

    fn set_number(&self, value: f64) {
        let value = value.clamp(0.0, self.maximum.get());
        self.change.borrow_mut()(V::from_number(value));
    }

    /// The modifier that lets keys and assistive technology read and move this range on a node of
    /// `role`. Increment and Decrement need no handler here. For a range with a step, the view
    /// turns them into a `SetValue` with the value moved by that step.
    pub(super) fn modifier(&self, role: Role) -> Modifier {
        let keyboard = self.clone();
        let set_value = self.clone();
        let current = self.value.clone();
        Modifier::new()
            .semantics(Semantics {
                role: Some(role),
                mode: SemanticsMode::MergeDescendants,
                range: Some(RangeInfo {
                    min: 0.0,
                    max: self.maximum.get(),
                    step: self.step.map(Positive::get),
                }),
                ..Semantics::default()
            })
            .on_accessibility_action(ActionKind::SetValue, move |action| {
                if let AccessibilityAction::SetValue(AccessibleValue::Number(value)) = action {
                    set_value.set_number(value);
                }
                Ok(())
            })
            .on_key(move |shortcut| keyboard.key(shortcut))
            .semantics_dynamic(move |semantics| {
                semantics.value = Some(AccessibleValue::Number(current()));
            })
    }

    /// Moves the value for the key of `shortcut` and returns whether the value moved. Returns
    /// `false` for a key pressed with Control or Alt, for an arrow or page key without a distance,
    /// and for every other key.
    fn key(&self, shortcut: Shortcut) -> bool {
        if shortcut.control || shortcut.alt {
            return false;
        }
        let current = (self.value)();
        let moved = |distance: Option<Positive>, sign: f64| {
            distance.map(|distance| current + sign * distance.get())
        };
        // Up and Page Up raise the value of a horizontal range. They lower the value of a vertical
        // range, because that value grows downward.
        let up = match self.axis {
            Axis::Horizontal => 1.0,
            Axis::Vertical => -1.0,
        };
        let value = match shortcut.key {
            Key::Home => Some(0.0),
            Key::End => Some(self.maximum.get()),
            Key::PageUp => moved(self.page, up),
            Key::PageDown => moved(self.page, -up),
            Key::Left => moved(self.step, -1.0),
            Key::Right => moved(self.step, 1.0),
            Key::Up => moved(self.step, up),
            Key::Down => moved(self.step, -up),
            _ => None,
        };
        let Some(value) = value else {
            return false;
        };
        self.set_number(value);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(maximum: f64) -> Result<RangeControl<f64>, NumberError> {
        RangeControl::new(|| 0.0, maximum, Axis::Horizontal, |_| {})
    }

    #[test]
    fn a_maximum_or_a_distance_that_is_not_finite_or_below_zero_is_refused() {
        assert_eq!(range(f64::INFINITY).err(), Some(NumberError::NotFinite));
        assert_eq!(range(-1.0).err(), Some(NumberError::Negative));
        let stepped = |step, page| range(1.0).and_then(|range| range.stepped(step, page));
        assert_eq!(
            stepped(Some(f64::NAN), None).err(),
            Some(NumberError::NotFinite)
        );
        assert_eq!(stepped(None, Some(-0.1)).err(), Some(NumberError::Negative));
        assert!(stepped(Some(0.0), Some(0.1)).is_ok_and(|range| range.step.is_none()));
    }
}

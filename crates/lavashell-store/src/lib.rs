// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use std::any::Any;

#[derive(Default)]
pub struct Store {
    layers: Vec<Box<dyn Any + Send>>,
}

impl Store {
    pub fn layer<L: Default + Send + 'static>(&mut self, add: impl FnOnce(&mut L)) {
        if let Some(layer) = self
            .layers
            .iter_mut()
            .find_map(|layer| layer.downcast_mut())
        {
            add(layer);
        } else {
            let mut layer = L::default();
            add(&mut layer);
            self.layers.push(Box::new(layer));
        }
    }

    #[must_use]
    pub fn get<L: 'static>(&self) -> Option<&L> {
        self.layers.iter().find_map(|layer| layer.downcast_ref())
    }
}

#[cfg(test)]
mod tests;

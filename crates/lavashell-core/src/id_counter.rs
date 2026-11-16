// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("every id is used")]
pub struct IdsExhausted;

#[derive(Default)]
pub struct IdCounter {
    last: Option<u64>,
}

impl IdCounter {
    pub fn next_id(&mut self) -> Result<u64, IdsExhausted> {
        let id = (self.last)
            .map_or(Some(0), |last| last.checked_add(1))
            .ok_or(IdsExhausted)?;
        self.last = Some(id);
        Ok(id)
    }
}

#[cfg(test)]
mod tests;

// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalRectangle {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl LogicalRectangle {
    pub fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    pub fn extent(self) -> Result<Extent, SessionError> {
        let length =
            |from: i32, to: i32| u32::try_from(to - from).map_err(SessionError::ExtentOutOfRange);
        Ok(Extent {
            width: length(self.left, self.right)?,
            height: length(self.top, self.bottom)?,
        })
    }
}

#[cfg(test)]
mod tests;

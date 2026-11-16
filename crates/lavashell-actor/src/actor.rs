// SPDX-License-Identifier: GPL-3.0-only
use crate::Merge;

pub trait Actor {
    type Input: Merge + Send + 'static;
    type Error: std::error::Error + Send + 'static;

    fn handle(&mut self, input: Self::Input) -> Result<Flow, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

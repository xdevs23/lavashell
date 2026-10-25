// SPDX-License-Identifier: GPL-3.0-only
use super::Role;

/// Where a native surface is in its life, with the role it has until it retires.
pub(super) enum Lifecycle {
    /// The compositor has not configured the surface yet.
    Waiting(Role),
    Configured(Role),
    /// The app or the compositor closed the surface, and the next frame retires it.
    Closing(Role),
    /// The surface released its resources and stays as the record that its index closed.
    Retired,
}

impl Lifecycle {
    /// Whether neither the app nor the compositor closed the surface.
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Waiting(_) | Self::Configured(_))
    }

    pub fn is_configured(&self) -> bool {
        matches!(self, Self::Configured(_))
    }

    pub fn is_closing(&self) -> bool {
        matches!(self, Self::Closing(_))
    }

    pub fn role(&self) -> Option<&Role> {
        match self {
            Self::Waiting(role) | Self::Configured(role) | Self::Closing(role) => Some(role),
            Self::Retired => None,
        }
    }

    pub fn role_mut(&mut self) -> Option<&mut Role> {
        match self {
            Self::Waiting(role) | Self::Configured(role) | Self::Closing(role) => Some(role),
            Self::Retired => None,
        }
    }

    /// Records the first configure of a waiting surface. A closed surface stays closed.
    pub fn configure(&mut self) {
        self.advance(|current| match current {
            Self::Waiting(role) => Self::Configured(role),
            other => other,
        });
    }

    /// Closes an open surface.
    pub fn close(&mut self) {
        self.advance(|current| match current {
            Self::Waiting(role) | Self::Configured(role) => Self::Closing(role),
            other => other,
        });
    }

    /// Retires the surface, and returns its role for release unless it retired already.
    pub fn retire(&mut self) -> Option<Role> {
        match std::mem::replace(self, Self::Retired) {
            Self::Waiting(role) | Self::Configured(role) | Self::Closing(role) => Some(role),
            Self::Retired => None,
        }
    }

    fn advance(&mut self, next: impl FnOnce(Self) -> Self) {
        let current = std::mem::replace(self, Self::Retired);
        *self = next(current);
    }
}

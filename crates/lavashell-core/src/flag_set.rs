// SPDX-License-Identifier: GPL-3.0-only

macro_rules! flag_set {
    (pub enum $flag:ident in $set:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $flag {
            $($variant),+
        }

        impl $flag {
            pub const ALL: [Self; [$(Self::$variant),+].len()] = [$(Self::$variant),+];

            const fn bit(self) -> u8 {
                1 << self as u8
            }
        }

        const _: () = assert!($flag::ALL.len() <= u8::BITS as usize);

        #[derive(Clone, Copy, Default, PartialEq, Eq)]
        pub struct $set(u8);

        impl $set {
            #[must_use]
            pub fn new(flags: impl IntoIterator<Item = $flag>) -> Self {
                Self(flags.into_iter().fold(0, |bits, flag| bits | flag.bit()))
            }

            #[must_use]
            pub fn all() -> Self {
                Self::new($flag::ALL)
            }

            #[must_use]
            pub const fn contains(self, flag: $flag) -> bool {
                self.0 & flag.bit() != 0
            }

            #[must_use]
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            pub fn iter(self) -> impl Iterator<Item = $flag> {
                $flag::ALL
                    .into_iter()
                    .filter(move |&flag| self.contains(flag))
            }
        }

        impl ::core::fmt::Debug for $set {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.debug_set().entries(self.iter()).finish()
            }
        }

        impl FromIterator<$flag> for $set {
            fn from_iter<I: IntoIterator<Item = $flag>>(flags: I) -> Self {
                Self::new(flags)
            }
        }
    };
    (pub enum $flag:ident in $set:ident { $($variant:ident => $named:ident),+ $(,)? }) => {
        $crate::flag_set::flag_set!(pub enum $flag in $set { $($variant),+ });

        impl $set {
            $(
                #[must_use]
                pub const fn $named(self) -> bool {
                    self.contains($flag::$variant)
                }
            )+
        }
    };
}

pub(crate) use flag_set;

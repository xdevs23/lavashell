// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

#[macro_export]
macro_rules! enum_with_values {
    (
        $(#[$attribute:meta])*
        $visibility:vis enum $name:ident {
            $($(#[$variant_attribute:meta])* $variant:ident $(= $discriminant:expr)?),+ $(,)?
        }

        $values_visibility:vis const ALL;
    ) => {
        $(#[$attribute])*
        $visibility enum $name {
            $($(#[$variant_attribute])* $variant $(= $discriminant)?),+
        }

        impl $name {
            $values_visibility const ALL: [Self; [$(Self::$variant),+].len()] =
                [$(Self::$variant),+];
        }
    };
}

#[cfg(test)]
mod tests;

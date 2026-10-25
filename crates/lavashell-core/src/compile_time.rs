// SPDX-License-Identifier: GPL-3.0-only

/// Unwraps a `Result` inside a `const` block, such as a checked number made from a literal. The compiler
/// evaluates the `Result` while it builds the program. An `Err` fails the build:
/// ```
/// # use lavashell_core::units::{Dp, NonNegativeDp};
/// const WIDTH: NonNegativeDp = lavashell_core::constant!(NonNegativeDp::try_new(Dp::new(2.0)));
/// ```
/// ```compile_fail
/// # use lavashell_core::units::{Dp, NonNegativeDp};
/// const WIDTH: NonNegativeDp = lavashell_core::constant!(NonNegativeDp::try_new(Dp::new(-2.0)));
/// ```
#[macro_export]
macro_rules! constant {
    ($result:expr $(,)?) => {
        const {
            match $result {
                ::core::result::Result::Ok(value) => value,
                ::core::result::Result::Err(_) => {
                    ::core::panic!("the constant is not a valid value of its type")
                }
            }
        }
    };
}

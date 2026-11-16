// SPDX-License-Identifier: GPL-3.0-only

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

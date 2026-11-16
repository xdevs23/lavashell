// SPDX-License-Identifier: GPL-3.0-only
use super::{PackageAlignment, PageSize, PageSizeError};

#[test]
fn reads_a_page_size_of_whole_kib() -> Result<(), PageSizeError> {
    let alignment = PackageAlignment {
        page_size: "16384".parse()?,
        file_alignment: 4,
    };
    assert_eq!(alignment.zipalign_arguments(), ["-P", "16", "4"]);
    Ok(())
}

#[test]
fn refuses_a_page_size_that_is_not_whole_kib_or_not_positive() {
    assert!(matches!(
        "1000".parse::<PageSize>(),
        Err(PageSizeError::NotWholeKib { .. })
    ));
    assert!(matches!(
        "0".parse::<PageSize>(),
        Err(PageSizeError::NotANumber(_))
    ));
    assert!(matches!(
        "16k".parse::<PageSize>(),
        Err(PageSizeError::NotANumber(_))
    ));
}

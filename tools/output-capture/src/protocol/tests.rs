// SPDX-License-Identifier: GPL-3.0-only
use super::{Reply, Request};
use crate::frame::{CapturedFrame, FrameLayout, PixelFormat};
use serde_json::error::Category;
use std::path::PathBuf;

#[test]
fn a_capture_line_names_the_image_file() -> Result<(), serde_json::Error> {
    assert_eq!(
        r#"{"command":"capture_change","image":"/run/frame.png"}"#.parse::<Request>()?,
        Request::CaptureChange {
            image: PathBuf::from("/run/frame.png")
        }
    );
    Ok(())
}

#[test]
fn a_line_that_is_no_request_is_rejected_with_the_category_of_its_flaw() {
    let flaws = [
        ("", Category::Eof),
        ("capture_change", Category::Syntax),
        (r#"{"command":"record"}"#, Category::Data),
        (r#"{"command":"capture_change"}"#, Category::Data),
        (
            r#"{"command":"capture_change","image":"/a","cursor":true}"#,
            Category::Data,
        ),
    ];
    for (line, category) in flaws {
        assert!(
            matches!(
                line.parse::<Request>(),
                Err(ref error) if error.classify() == category
            ),
            "{line:?} was not rejected as {category:?}"
        );
    }
}

#[test]
fn every_reply_is_one_json_object_tagged_by_its_kind() -> Result<(), serde_json::Error> {
    let frame = CapturedFrame {
        layout: FrameLayout {
            format: PixelFormat::Xrgb8888,
            width: 1280,
            height: 720,
            stride: 5120,
        },
        inverted: false,
    };
    let replies = [
        (
            Reply::from(frame),
            r#"{"reply":"frame","width":1280,"height":720,"stride":5120,"format":"xrgb8888","inverted":false}"#,
        ),
        (
            Reply::Failed {
                message: "no output".to_owned(),
            },
            r#"{"reply":"failed","message":"no output"}"#,
        ),
    ];
    for (reply, line) in replies {
        assert_eq!(serde_json::to_string(&reply)?, line);
        assert_eq!(line.parse::<Reply>()?, reply);
    }
    Ok(())
}

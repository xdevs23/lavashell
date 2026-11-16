// SPDX-License-Identifier: GPL-3.0-only
use super::{Reply, Request};
use crate::error::SessionError;
use serde_json::error::Category;

#[test]
fn a_request_line_becomes_the_request_it_names() -> Result<(), SessionError> {
    assert_eq!(
        Request::parse(r#"{"command":"move","x":100,"y":200}"#)?,
        Request::Move { x: 100, y: 200 }
    );
    assert_eq!(
        Request::parse(r#"{"command":"key","name":"SPCE"}"#)?,
        Request::Key {
            name: "SPCE".to_owned()
        }
    );
    Ok(())
}

#[test]
fn a_line_that_is_no_request_is_rejected_with_the_category_of_its_flaw() {
    let flaws = [
        ("", Category::Eof),
        ("move 100 100", Category::Syntax),
        (r#"{"command":"jump"}"#, Category::Data),
        (r#"{"command":"move","x":1}"#, Category::Data),
        (r#"{"command":"move","x":-1,"y":0}"#, Category::Data),
        (r#"{"command":"key","name":"SPCE","x":1}"#, Category::Data),
    ];
    for (line, category) in flaws {
        assert!(
            matches!(
                Request::parse(line),
                Err(SessionError::ParseRequest(ref error)) if error.classify() == category
            ),
            "{line:?} was not rejected as {category:?}"
        );
    }
}

#[test]
fn every_reply_is_one_json_object_tagged_by_its_kind() -> Result<(), serde_json::Error> {
    let replies = [
        (Reply::Ready { pid: 42 }, r#"{"reply":"ready","pid":42}"#),
        (Reply::Done, r#"{"reply":"done"}"#),
        (
            Reply::Failed {
                message: "no key".to_owned(),
            },
            r#"{"reply":"failed","message":"no key"}"#,
        ),
    ];
    for (reply, line) in replies {
        assert_eq!(serde_json::to_string(&reply)?, line);
    }
    Ok(())
}

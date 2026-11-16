// SPDX-License-Identifier: GPL-3.0-only
use super::{Reply, Request};
use serde_json::error::Category;

#[test]
fn a_request_line_becomes_the_request_it_names() -> Result<(), serde_json::Error> {
    assert_eq!(
        r#"{"command":"move","x":100,"y":200}"#.parse::<Request>()?,
        Request::Move { x: 100, y: 200 }
    );
    assert_eq!(
        r#"{"command":"key","name":"SPCE"}"#.parse::<Request>()?,
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
                line.parse::<Request>(),
                Err(ref error) if error.classify() == category
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
        assert_eq!(line.parse::<Reply>()?, reply);
    }
    Ok(())
}

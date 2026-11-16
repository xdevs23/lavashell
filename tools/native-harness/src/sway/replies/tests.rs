// SPDX-License-Identifier: GPL-3.0-only
use super::{Node, SwayWindow};

#[test]
fn the_windows_of_a_tree_include_tiled_and_floating_nodes_in_tree_order()
-> Result<(), serde_json::Error> {
    let tree: Node = serde_json::from_str(
        r#"{"nodes": [
              {"nodes": [{"nodes": [], "floating_nodes": [], "app_id": "counter", "pid": 41}],
               "floating_nodes": [{"nodes": [], "floating_nodes": [], "app_id": "taskbar", "pid": 42}]}
            ],
            "floating_nodes": []}"#,
    )?;
    let window = |app_id: &str, pid| SwayWindow {
        app_id: app_id.to_owned(),
        pid,
    };
    assert_eq!(
        tree.windows(),
        [window("counter", 41), window("taskbar", 42)]
    );
    Ok(())
}

#[test]
fn a_node_without_an_app_id_or_a_pid_is_no_window() -> Result<(), serde_json::Error> {
    let tree: Node = serde_json::from_str(
        r#"{"nodes": [
              {"nodes": [], "floating_nodes": [], "app_id": null, "pid": 7},
              {"nodes": [], "floating_nodes": [], "app_id": "x"}
            ],
            "floating_nodes": [], "pid": 1}"#,
    )?;
    assert_eq!(tree.windows(), []);
    Ok(())
}

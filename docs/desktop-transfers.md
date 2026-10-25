# Desktop transfers

## Clipboard

```rust
let item = TransferData::new([
    Representation::new("text/plain;charset=utf-8", plain_text)?,
    Representation::new("text/html", html_bytes)?,
])?;
clipboard.set_data(item, |result| {
    // Handle submission failure in application state.
});

clipboard.read(surface, ["image/png", "text/plain;charset=utf-8"], |result| {
    // `result` holds the MIME type and the bytes of the first offered format.
});
```

## Drop target

```rust
let app = app
    .on_drag(|event, view| match event {
        DragEvent::Enter(info) | DragEvent::Motion(info) => {
            let over = view.bounds_of("target").is_some_and(|r| r.contains(info.position));
            over.then(|| DropAcceptance { mime_type: "image/png".into(), action: DragAction::Copy })
        }
        _ => None,
    })
    .on_drop(|event, _| event.data.is_ok());
```

## Drag source

```rust
let app = app.on_drag_source(move |gesture, view| {
    let button = view.bounds_of("source")?;
    if !button.contains(gesture.origin) || button.contains(gesture.position) {
        return None;
    }
    let data = TransferData::single("image/png", png.clone()).ok()?;
    Some(DragRequest::new(data, DragActions::new([DragAction::Copy]), |result, _| {
        // Apply a Move to the source data only after `result` reports it.
    }))
});
```

Native drag completion needs `wl_data_device_manager` version 3.

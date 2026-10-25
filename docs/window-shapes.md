# Window shapes

```rust
let window = Window::new("Shaped", "org.example.Shaped")
    .transparent(true)
    .shape(WindowShape::new(|size| {
        let (w, h) = (size.width, size.height);
        vec![
            vec![Point::new(0.dp(), 0.dp()), Point::new(w, 0.dp()), Point::new(w, h), Point::new(0.dp(), h)],
            vec![Point::new(40.dp(), 40.dp()), Point::new(80.dp(), 40.dp()), Point::new(60.dp(), 80.dp())],
        ]
    }));

let rounded = WindowShape::rounded_rect(|size| Some((Rect::from_size(size), 24.dp())));
```

Lavashell fills contours with the even-odd rule, so the second contour cuts a hole. The surface below receives pointer
and touch input outside the shape.

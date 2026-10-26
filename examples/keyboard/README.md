# On-screen keyboard

A car screen with a music search field and the on-screen keyboard docked at the bottom. A pinch in on the keyboard
moves it to the left corner. A pinch out gives it the full width again.

```sh
nix run .#keyboard
nix run .#keyboard -- --day
nix run .#keyboard -- --frames <directory> --frame-rate 60
```

Options: `--width`, `--height`, `--text-scale`, `--reduced-motion`, `--reduced-transparency`, `--fullscreen`,
`--exit-after`.

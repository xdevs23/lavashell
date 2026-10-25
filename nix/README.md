# Nix build layers

| Layer | Source inputs | Stored artifacts |
| --- | --- | --- |
| Native registry | Projected native manifests and lock | External native crates |
| Engine registry | Engine dependencies and their resolved shared features | External engine crates |
| Framework | Local crate source, with the final package stubbed | Native framework or engine adapter variant |
| Example | Actual package source | Runnable binary and Cargo build record |

## Projections

Refresh the projections after changing a manifest or the lock file. Fetch the registry sources first.

```sh
cargo fetch
nix develop --command python3 nix/refresh-cargo-projections.py
```

## Checks

```sh
python3 nix/test-cargo-reuse.py
python3 nix/test-dependency-workspace.py
python3 nix/test-layered-progress.py
nix develop --command python3 nix/test-cargo-projections.py
nix develop --command python3 nix/test-layer-boundaries.py
nix develop --command python3 nix/test-dependency-boundaries.py
nix develop --command python3 nix/test-cargo-hooks.py
nix develop --command python3 nix/test-cache-copy.py
nix develop --command python3 nix/test-cargo-patch.py
nix develop --command python3 nix/test-cargo-vendor.py
nix develop --command python3 nix/test-skia.py
```

## Dependencies

Record each accepted advisory and the reason for accepting it in `dependency-check.nix`.

```sh
nix run .#yanked-crates
nix flake update advisory-db
```

## Sandbox reuse

```sh
OUT=$(mktemp -d)
nix develop --command python3 nix/verify-layered-build.py --output "$OUT"
```

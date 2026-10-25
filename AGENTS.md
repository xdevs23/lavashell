# Repository rules

Every runnable example has a flake package and an app and runs on NixOS with `nix run` alone.
Build tools, runtime libraries and assets are declared through Nix.
A packaging change is complete once each package builds in the sandbox and launches natively.

Implementation files are shorter than 500 lines, and no line is longer than 120 characters.
Each subsystem sits behind a module boundary and registers itself independently.

Code outside tests does not call `unwrap` or `expect` and does not turn an error into a string. Every failure reaches
its caller as a typed error, through `?` or a `From` conversion that keeps its source.

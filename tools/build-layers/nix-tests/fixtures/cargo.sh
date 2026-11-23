#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
case "$*" in
    *--message-format=json*) message() { printf '%s\n' "$1"; } ;;
    *) message() { :; } ;;
esac
if [ "$CARGO_FIXTURE_STATUS" != 0 ]; then
    message '{"reason":"build-finished","success":false}'
    exit "$CARGO_FIXTURE_STATUS"
fi
mkdir -p target/x86_64-unknown-linux-gnu/packaged
printf '#!/bin/sh\nexit 0\n' > target/x86_64-unknown-linux-gnu/packaged/fixture
chmod +x target/x86_64-unknown-linux-gnu/packaged/fixture
printf '   Compiling %s\n' "$1" >&2
message "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"$1\"}}"
message '{"reason":"build-finished","success":true}'
if [ "$1" = test ]; then
    printf '%s\n' '{"reason":"compiler-artifact","target":{"name":"printed-by-a-test"}}'
fi

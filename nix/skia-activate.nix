# SPDX-License-Identifier: GPL-3.0-only
# The build script of skia-bindings copies the Skia libraries into OUT_DIR. When the script runs again, it cannot
# overwrite copies that are read-only like the files in the Nix store. A build outside the sandbox therefore links
# writable copies of the libraries, made in the cache directory of the user.
{
  name,
  skia,
  storeDir,
}:
''
  . ${skia}/nix-support/skia-environment
  copies="''${XDG_CACHE_HOME:-$HOME/.cache}/lavashell/${name}"
  export SKIA_LIBRARY_SEARCH_PATH="$copies/${baseNameOf skia}"
  if [ ! -d "$SKIA_LIBRARY_SEARCH_PATH" ]; then
    mkdir -p "$copies"
    copy=$(mktemp -d "$copies/.XXXXXX")
    if install -m644 ${skia}/lib/*.a "$copy"; then
      mv -T "$copy" "$SKIA_LIBRARY_SEARCH_PATH" 2>/dev/null || true
    fi
    rm -rf "$copy"
    if [ ! -d "$SKIA_LIBRARY_SEARCH_PATH" ]; then
      echo "The Skia libraries could not be copied to $SKIA_LIBRARY_SEARCH_PATH." \
        "Free disk space, make the directory writable or set XDG_CACHE_HOME to a writable directory." \
        "Then start again." >&2
      return 1 2>/dev/null || exit 1
    fi
  fi
  for copied in "$copies"/*; do
    if [ ! -e "${storeDir}/$(basename "$copied")" ]; then
      rm -rf "$copied"
    fi
  done
''

# SPDX-License-Identifier: GPL-3.0-only
{ lib }:
let
  stack = layers: lib.unique (lib.concatMap (layer: stack layer.previous ++ [ layer ]) layers);
  removedPaths = "share/lavashell/removed-target-paths";
  lowerTarget = ''"$NIX_BUILD_TOP/lavashell-lower-target"'';
  layerTarget = ''"$NIX_BUILD_TOP/lavashell-layer-target"'';
  listTarget = list: ''
    find target ! -type d -printf '%P\t%s\t%T@\0' | LC_ALL=C sort -z > ${list}
  '';
  pathsIn = list: "<(cut -z -f1 ${list} | LC_ALL=C sort -z)";
  restore = layers: ''
    restoreArtifactLayer() {
      cp -a --no-preserve=links --reflink=auto "$1/target/." target/
      (cd "$1/target" && find . -print0) | (cd target && xargs -0 chmod u+w)
      (cd target && xargs -0 -r rm -f -- < "$1/${removedPaths}")
    }
    mkdir -p target
    ${lib.concatMapStrings (layer: "restoreArtifactLayer ${layer}\n") (stack layers)}
  '';
in
{
  inherit stack restore;
  buildRecord = "share/lavashell/cargo-artifacts.jsonl";
  restoreLowerLayers = previous: restore previous + listTarget lowerTarget;
  storeLayerChanges = ''
    mkdir -p "$out/target" "$out/${dirOf removedPaths}"
  ''
  + listTarget layerTarget
  + ''
    LC_ALL=C comm -z -13 ${lowerTarget} ${layerTarget} | cut -z -f1 \
      | tar --create --directory=target --null --files-from=- --file=- \
      | tar --extract --directory="$out/target" --file=-
    LC_ALL=C comm -z -23 ${pathsIn lowerTarget} ${pathsIn layerTarget} > "$out/${removedPaths}"
  '';
}

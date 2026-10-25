# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  sources,
  root,
}:
let
  inherit (pkgs) lib;
  template = pkgs.writeText "notices.hbs" ''
    {{#each licenses}}
    ================================================================================
    {{name}}

    Used by:
    {{#each used_by}}
      {{crate.name}} {{crate.version}}{{#if crate.repository}} ({{crate.repository}}){{/if}}
    {{/each}}

    {{{text}}}

    {{/each}}
  '';
in
{
  directory,
  member ? directory,
  target ? pkgs.stdenv.hostPlatform.rust.rustcTarget,
}:
pkgs.stdenv.mkDerivation {
  name = "lavashell-notices-${baseNameOf member}";
  src = sources.sourceFor directory;
  cargoDeps = sources.vendor (sources.project directory).lockFile;
  nativeBuildInputs = with pkgs; [
    rustPlatform.cargoSetupHook
    cargo
    cargo-about
    rustc
  ];
  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;
  installPhase = ''
    cargo about generate --frozen --fail --target ${target} \
      --config ${root + "/about.toml"} \
      --manifest-path ${lib.removePrefix "${toString root}/" (toString member)}/Cargo.toml \
      --output-file "$out" ${template}
  '';
}

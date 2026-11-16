# SPDX-License-Identifier: GPL-3.0-only
let
  types = import ./types.nix;
in
{
  aarch64-linux-android = {
    abi = "arm64-v8a";
    architecture = "aarch64";
    role = types.targetRoles.device;
    skia = {
      # https://gn.googlesource.com/gn/+/main/docs/reference.md#var_target_cpu
      target_cpu = "arm64";
      # https://clang.llvm.org/docs/ClangCommandLineReference.html#cmdoption-clang-mno-outline-atomics
      extra_cflags = [ "-mno-outline-atomics" ];
    };
  };
  x86_64-linux-android = {
    abi = "x86_64";
    architecture = "x86_64";
    role = types.targetRoles.emulator;
    # https://gn.googlesource.com/gn/+/main/docs/reference.md#var_target_cpu
    skia.target_cpu = "x64";
  };
}

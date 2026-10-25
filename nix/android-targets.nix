# SPDX-License-Identifier: GPL-3.0-only
{
  aarch64-linux-android = {
    abi = "arm64-v8a";
    skia = {
      target_cpu = "arm64";
      extra_cflags = [ "-mno-outline-atomics" ];
    };
  };
  x86_64-linux-android = {
    abi = "x86_64";
    skia.target_cpu = "x64";
  };
}

# SPDX-License-Identifier: GPL-3.0-only
''
  find "$NIX_BUILD_TOP/$(stripHash "$cargoDeps")" -depth -type d -exec touch -h -r "$cargoDeps" {} +
''

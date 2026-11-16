# SPDX-License-Identifier: GPL-3.0-only
{ lib }:
previous:
lib.optionalString (previous != [ ]) ''
  mkdir -p target
  ${lib.concatMapStrings (cache: ''
    cp -a --no-preserve=links --reflink=auto ${cache}/target/. target/
    chmod -R u+w target
  '') previous}
''

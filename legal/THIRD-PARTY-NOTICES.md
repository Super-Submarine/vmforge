# VMForge — Third-Party Notices

This file lists the third-party software whose code is compiled into, or
otherwise distributed with, the VMForge wave-1 Linux artifacts
(`vmforge_<ver>_amd64.deb`, `vmforge-<ver>-x86_64.AppImage`), together with
the required attribution and license texts.

Generated against `Cargo.lock` at the commit this file was introduced.
Regenerate whenever `Cargo.lock` changes (policy: `cargo about generate`,
see the license-compliance CI spec, company doc `019f8a6d-8dcb`).

Where a component is offered under `MIT OR Apache-2.0`, VMForge elects
**Apache-2.0** (aligned with the planned open-core license). For
`memchr` (`MIT OR Unlicense`) VMForge elects **MIT**. Elections do not
remove `AND` terms: the Unicode-3.0 notice for `unicode-ident` is
reproduced regardless of election.

## Section A — Rust crates statically linked into the `vmforge` binary

| Crate | Version | License (SPDX) | Election | Copyright |
|---|---|---|---|---|
| itoa | 1.0.18 | MIT OR Apache-2.0 | Apache-2.0 | © David Tolnay |
| memchr | 2.8.3 | MIT OR Unlicense | MIT | © 2015 Andrew Gallant |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | Apache-2.0 | © David Tolnay, Alex Crichton |
| quote | 1.0.47 | MIT OR Apache-2.0 | Apache-2.0 | © David Tolnay |
| serde | 1.0.229 | MIT OR Apache-2.0 | Apache-2.0 | © Erick Tryzelaar, David Tolnay |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | Apache-2.0 | © Erick Tryzelaar, David Tolnay |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | Apache-2.0 | © Erick Tryzelaar, David Tolnay |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Apache-2.0 | © Erick Tryzelaar, David Tolnay |
| syn | 2.0.119 | MIT OR Apache-2.0 | Apache-2.0 | © David Tolnay |
| syn | 3.0.3 | MIT OR Apache-2.0 | Apache-2.0 | © David Tolnay |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | Apache-2.0 | © David Tolnay |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | Apache-2.0 | © David Tolnay |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | Apache-2.0 AND Unicode-3.0 | © David Tolnay; Unicode data © Unicode, Inc. |
| zmij | 1.0.23 | MIT | MIT | © David Tolnay |

Notes:
- `proc-macro2`, `quote`, `syn` (both versions), and `unicode-ident` are
  procedural-macro/build-time dependencies; the code that
  `serde_derive`/`thiserror-impl` *generate* is compiled into the shipped
  binary, so all are listed for completeness.
- License sources: https://crates.io/crates/<name> for each crate;
  Unicode-3.0: https://spdx.org/licenses/Unicode-3.0.html

### Rust standard library

The compiled `vmforge` binaries statically embed the Rust standard library,
licensed `MIT OR Apache-2.0` (elected: Apache-2.0).
© The Rust Project Developers / The Rust Project contributors.
https://github.com/rust-lang/rust (COPYRIGHT, LICENSE-MIT, LICENSE-APACHE)

## Section B — Components used but NOT redistributed in wave-1 artifacts

Listed for transparency; no distribution obligations attach because VMForge
does not convey these binaries in wave 1:

| Component | License | Relationship |
|---|---|---|
| QEMU (`qemu-system-x86`, `qemu-img`) | GPL-2.0 (https://www.qemu.org/docs/master/about/license.html) | Invoked strictly as separate host processes (QMP / CLI). The `.deb` declares `Depends: qemu-system-x86`; the `.AppImage` requires host-installed QEMU. Not linked, not bundled. QEMU is a trademark of Fabrice Bellard. |
| Linux KVM | GPL-2.0 with Linux-syscall-note | Host kernel API via `/dev/kvm` ioctls; userspace use does not propagate GPL (https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/LICENSES/exceptions/Linux-syscall-note) |

## Section C — Packaging-tool runtime (AppImage)

Every AppImage embeds the AppImage **type2-runtime** produced by
`appimagetool` (pinned 1.9.1). The runtime is MIT-licensed
(https://github.com/AppImage/type2-runtime/blob/main/LICENSE); attribution
is included below under the MIT text.

---

## License texts

### Apache License 2.0

Applies to crates elected Apache-2.0 above and the Rust standard library.
Full text: https://www.apache.org/licenses/LICENSE-2.0

    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use these files except in compliance with the License.
    You may obtain a copy of the License at

        http://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.

### MIT License

Applies to: memchr (© 2015 Andrew Gallant), zmij (© David Tolnay),
AppImage type2-runtime (© the AppImage authors).

    Permission is hereby granted, free of charge, to any person obtaining
    a copy of this software and associated documentation files (the
    "Software"), to deal in the Software without restriction, including
    without limitation the rights to use, copy, modify, merge, publish,
    distribute, sublicense, and/or sell copies of the Software, and to
    permit persons to whom the Software is furnished to do so, subject to
    the following conditions:

    The above copyright notice and this permission notice shall be
    included in all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
    EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
    MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
    NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
    LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
    OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
    WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### Unicode License v3 (Unicode-3.0)

Applies to the Unicode data tables embedded via `unicode-ident`.
Copyright © 1991-2025 Unicode, Inc.

    UNICODE LICENSE V3

    COPYRIGHT AND PERMISSION NOTICE

    Copyright © 1991-2025 Unicode, Inc.

    NOTICE TO USER: Carefully read the following legal agreement. BY
    DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
    SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
    TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
    DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of data files and any associated documentation (the "Data Files") or
    software and any associated documentation (the "Software") to deal in the
    Data Files or Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, and/or sell
    copies of the Data Files or Software, and to permit persons to whom the
    Data Files or Software are furnished to do so, provided that either (a)
    this copyright and permission notice appear with all copies of the Data
    Files or Software, or (b) this copyright and permission notice appear in
    associated Documentation.

    THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
    KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
    MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
    THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS
    INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT
    OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
    OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
    OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
    PERFORMANCE OF THE DATA FILES OR SOFTWARE.

    Except as contained in this notice, the name of a copyright holder shall
    not be used in advertising or otherwise to promote the sale, use or other
    dealings in these Data Files or Software without prior written
    authorization of the copyright holder.

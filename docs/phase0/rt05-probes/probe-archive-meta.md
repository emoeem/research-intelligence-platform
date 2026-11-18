# RT-0.5 盘点原始取证 · 归档与元数据（0.5-A/B）

- 盘点员子代理实测，只读；主机 CachyOS (linux 7.3.4-1-cachyos-bore-lto, x86_64)；2026-11-18
- 未解包任何归档；每条探测命令 timeout 5s

## archive.7z

| 字段 | 值 |
|---|---|
| capabilityId | archive.7z |
| executablePath | `/usr/bin/7z`（command -v → `/usr/sbin/7z`，readlink -f 归一；`/usr/sbin` 为 `/usr/bin` 同目录符号链接） |
| version | 7-Zip 26.06 (x64)，Igor Pavlov，2026-11-06 |
| packageProvenance | `7zip 26.06-1.1`（pacman -Qo） |
| binaryIdentity | `f802383f13b8331a` + `POSIX shell script, ASCII text executable`（包装脚本）；真实 ELF `/usr/lib/7zip/7z`：sha256 `d48e622e9009acdb`，`ELF 64-bit LSB pie executable, x86-64, stripped` |
| permissions | `root:root 755` |
| status | available |
| capabilityProbe | `7z i` 见 evidence #6/#7；Libs: `0 : 26.06 : /usr/lib/7zip/7z.so`；格式支持摘要：**7z / zip / tar / Rar / Rar5 / gzip / xz / bzip2 / zstd 全部在列**，另有 APFS/APM/Ar/Arj/Cab/Chm/Compound/Cpio/CramFS/Dmg/ELF/Ext/FAT/ISO/HFS/NTFS/NSIS/PE 等约 30 种识别格式 |

## archive.7za

| 字段 | 值 |
|---|---|
| capabilityId | archive.7za |
| executablePath | `/usr/bin/7za`（command -v → `/usr/sbin/7za`） |
| version | 7-Zip (a) 26.06 (x64) |
| packageProvenance | `7zip 26.06-1.1` |
| binaryIdentity | `4566c6e82f8147dc` + `POSIX shell script, ASCII text executable`（包装脚本，exec `/usr/lib/7zip/7za`） |
| permissions | `root:root 755` |
| status | available |
| capabilityProbe | `7za i` 前 20 行：Formats = 7z / Cab / Split / bzip2 / gzip / lzma / lzma86 / tar / xz / zip / zstd / Hash。**7z/zip/tar 在列；Rar 不在列**（独立构建无 Rar 解码） |

## archive.7zr

| 字段 | 值 |
|---|---|
| capabilityId | archive.7zr |
| executablePath | `/usr/bin/7zr`（command -v → `/usr/sbin/7zr`） |
| version | 7-Zip (r) 26.06 (x64)，Public domain |
| packageProvenance | `7zip 26.06-1.1` |
| binaryIdentity | `1c5de14f5ac7ac74` + `POSIX shell script, ASCII text executable`（包装脚本，exec `/usr/lib/7zip/7zr`） |
| permissions | `root:root 755` |
| status | available |
| capabilityProbe | `7zr i` 前 20 行：Formats = 7z / Split / lzma / lzma86 / xz / Hash；Codecs = AES256CBC / 7zAES / LZMA / LZMA2 / Delta / Copy。**仅 7z 原生格式，无 zip/tar/rar** |

## archive.bsdtar

| 字段 | 值 |
|---|---|
| capabilityId | archive.bsdtar |
| executablePath | `/usr/bin/bsdtar`（command -v → `/usr/sbin/bsdtar`） |
| version | bsdtar 3.8.11 - libarchive 3.8.11 zlib/1.3.1.zlib-ng liblzma/5.8.3 bz2lib/1.0.8 liblz4/1.11.1 libzstd/1.5.7 libxml2/2.15.4 openssl/4.1.0 libb2/bundled libacl/2.4.0 |
| packageProvenance | `libarchive 3.8.11-2.1` |
| binaryIdentity | `0081c8f3e9820eae` + `ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped` |
| permissions | `root:root 755` |
| status | available |
| capabilityProbe | `--version` 正常；**`--formats` 选项不存在**（`bsdtar: Option --formats is not supported`，exit 1）→ 无格式清单子命令；create 模式 `--format {ustar|pax|cpio|shar}`（来自 --help） |

## archive.unzip

| 字段 | 值 |
|---|---|
| capabilityId | archive.unzip |
| executablePath | `/usr/bin/unzip`（command -v → `/usr/sbin/unzip`） |
| version | UnZip 6.00 of 20 April 2009, by Info-ZIP |
| packageProvenance | `unzip 6.0-23.1` |
| binaryIdentity | `15f733fefed8de50` + `ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped` |
| permissions | `root:root 755` |
| status | available |
| capabilityProbe | 无参数用法首行：`UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.` |

## meta.exiftool

| 字段 | 值 |
|---|---|
| capabilityId | meta.exiftool |
| executablePath | `/usr/bin/vendor_perl/exiftool`（command -v 直接命中，readlink -f 无变化） |
| version | 13.55（`-ver`） |
| packageProvenance | `perl-image-exiftool 13.55-1` |
| binaryIdentity | `80e0018c48f93244` + `Perl script text executable` |
| permissions | `root:root 555` |
| status | available |
| capabilityProbe | `exiftool -ver` → `13.55`，与包版本一致 |

## missing 清单

无。六项能力全部 available，无可执行文件缺失。

## anomalies 清单

1. **7z/7za/7zr 均为 sh 包装脚本而非 ELF**：`#!/bin/sh` + `exec /usr/lib/7zip/{7z,7za,7zr} "$@"`。binaryIdentity 字段记录的是包装脚本哈希；真实 ELF 在 `/usr/lib/7zip/`（`7z`、`7za`、`7zr`、`7zCon.sfx`、`7z.so` 五个文件），`archive.7z` 的真实 ELF sha256 前 16 位为 `d48e622e9009acdb`。
2. **p7zip 与 7-zip 包区分**：本机未安装 p7zip 独立包；`pacman -Q p7zip` 经 provides/虚拟依赖解析到 `7zip 26.06-1.1`。banner 为 `7-Zip 26.06`（Igor Pavlov 官方 Linux 移植），不是 p7zip 17.x（其 banner 应为 `7-Zip (a) 17.05`）。包归属统一为 `7zip 26.06-1.1`。
3. **archive.7za 无 Rar 支持**：`7za i` 格式清单中无 Rar/Rar5 行（独立构建），Rar 解码能力仅在 `archive.7z`（经 `/usr/lib/7zip/7z.so` 外挂编解码库）上存在。
4. **bsdtar 无 `--formats` 选项**：格式清单探测不适用，实际返回 `bsdtar: Option --formats is not supported`；可用格式声明仅为 create 模式 `--format {ustar|pax|cpio|shar}`，读侧格式集由 libarchive 3.8.11 提供（版本行可见 zlib/lzma/bz2/lz4/zstd/xml2/openssl 后端）。
5. **exiftool 权限 555**，与其余 755 不同（perl-image-exiftool 包的正常行为），且为 Perl 脚本而非 ELF。
6. `/usr/sbin/` 下同名条目均为 `/usr/bin` 归一路径（`readlink -f` 已归一），不是独立副本。

## evidence 清单（命令原文 → 关键回显）

1. `timeout 5 bash -c 'command -v 7z'` → `/usr/sbin/7z`；`timeout 5 readlink -f /usr/sbin/7z` → `/usr/bin/7z`（7za/7zr/bsdtar/unzip 同型）
2. `timeout 5 /usr/bin/7z | head -10` → `7-Zip 26.06 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-11-06`
3. `timeout 5 pacman -Qo /usr/bin/7z` → `/usr/bin/7z 由 7zip 26.06-1.1 所拥有`（7za/7zr 同包；bsdtar → `libarchive 3.8.11-2.1`；unzip → `unzip 6.0-23.1`；exiftool → `perl-image-exiftool 13.55-1`）
4. `timeout 5 sha256sum /usr/bin/7z` → `f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4`；`timeout 5 file -b /usr/bin/7z` → `POSIX shell script, ASCII text executable`
5. `timeout 5 head -20 /usr/bin/7z` → `#!/bin/sh` / `exec /usr/lib/7zip/7z "$@"`（7za→`/usr/lib/7zip/7za`，7zr→`/usr/lib/7zip/7zr`）
6. `timeout 5 /usr/bin/7z i | head -20` → `Libs: 0 : 26.06 : /usr/lib/7zip/7z.so` + Formats 首段（7z/APFS/APM/Ar/Arj/Base64/COFF/Cab/Chm/Compound/Cpio...）
7. `timeout 5 /usr/bin/7z i | grep -Eiw '(7z|zip|tar|rar|xz|gzip|bzip2)' | head -40` → `7z`、`Rar`、`Rar5`、`tar`、`zip`、`gzip`、`xz`、`bzip2`、`zstd` 行全部命中
8. `timeout 5 /usr/bin/7za i | head -20` → `7-Zip (a) 26.06`，Formats 含 `zip`/`tar`/`xz`，无 `Rar` 行；`timeout 5 /usr/bin/7zr i | head -20` → `7-Zip (r) 26.06`，Formats 仅 `7z/Split/lzma/lzma86/xz/Hash`
9. `timeout 5 /usr/bin/bsdtar --version` → `bsdtar 3.8.11 - libarchive 3.8.11 zlib/1.3.1.zlib-ng liblzma/5.8.3 ... openssl/4.1.0 libb2/bundled libacl/2.4.0`
10. `timeout 5 /usr/bin/bsdtar --formats` → exit 1，`bsdtar: Option --formats is not supported`
11. `timeout 5 /usr/bin/unzip | head -3` → `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.`
12. `timeout 5 /usr/bin/vendor_perl/exiftool -ver` → `13.55`
13. `timeout 5 pacman -Q p7zip` → `7zip 26.06-1.1`（p7zip 无独立包，虚拟解析）
14. `timeout 5 stat -c '%U:%G %a' /usr/bin/7z` → `root:root 755`（7za/7zr/bsdtar/unzip 同；exiftool → `root:root 555`）

## 复现

```bash
for x in 7z 7za 7zr bsdtar unzip; do p=$(readlink -f "$(command -v $x)"); echo "$x: $p | $(pacman -Qo $p) | $(sha256sum $p | cut -c1-16) | $(stat -c '%U:%G %a' $p)"; done
p=$(readlink -f "$(command -v exiftool)"); echo "exiftool: $p | $(pacman -Qo $p) | $(sha256sum $p | cut -c1-16) | $(stat -c '%U:%G %a' $p) | ver=$($p -ver)"
/usr/bin/7z i | grep -Eiw '(7z|zip|tar|rar|xz|gzip|bzip2)'; /usr/bin/bsdtar --formats; /usr/bin/unzip | head -1
```
预期回显：六行均为 `capabilityId: 路径 | <包名 版本> | <sha256 前16位> | 权限`；exiftool 行追加 `ver=13.55`；格式段出现 `7z/Rar/Rar5/zip/tar/...` 行；`/usr/bin/bsdtar --formats` 报 `Option --formats is not supported`；unzip 首行为 `UnZip 6.00 of 20 April 2009, by Info-ZIP.`

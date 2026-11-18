# RT-0.5 盘点原始取证 · Office 与文档转换（0.5-A/B）

- 盘点员子代理实测，只读；host: CachyOS, kernel 7.3.4-1-cachyos-bore-lto；2026-11-18
- 未启动 soffice 实例、未执行任何转换；每条探测命令 timeout 5s

## document.libreoffice

| 字段 | 值 |
|---|---|
| capabilityId | document.libreoffice |
| executablePath | `command -v` → `/usr/sbin/soffice`；`readlink -f` → `/usr/lib/libreoffice/program/soffice`（POSIX shell wrapper）；真实二进制 → `/usr/lib/libreoffice/program/soffice.bin` |
| version | `LibreOffice 26.8.2.2 680(Build:2)` |
| packageProvenance | `libreoffice-fresh 26.8.2-3`（wrapper 与 soffice.bin 均归属该包） |
| binaryIdentity | soffice.bin: `baa182393db76be0` · `ELF 64-bit LSB pie executable, x86-64, dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2, BuildID[sha1]=6c007680…, stripped`；wrapper: `3d585422d7cdddf9` · `POSIX shell script, ASCII text executable` |
| permissions | wrapper `root:root 755`；soffice.bin `root:root 755` |
| status | `available` |
| capabilityProbe | `--version` 正常输出版本串；program 目录组件面齐：Writer `libswlo.so`+`libswuilo.so`、Calc `libsclo.so`+`libscuilo.so`、Impress `libsdlo.so`+`libsduilo.so`、`soffice.bin` 全部存在；`--help` 第 57 行含 `--headless  Starts in "headless mode"…`（且第 120/142/144 行显示 `--convert-to`/`--print-to-file`/`--cat` 均 implies `--headless`）→ headless 可用（仅凭 --help 判定，未启动实例） |

## convert.pandoc

| 字段 | 值 |
|---|---|
| capabilityId | convert.pandoc |
| executablePath | `command -v` → `/usr/sbin/pandoc`；`readlink -f` → `/usr/bin/pandoc`（无 wrapper） |
| version | `pandoc 3.11`（Features: `+server +lua`，Scripting engine: Lua 5.4） |
| packageProvenance | `pandoc-cli 3.11-87` |
| binaryIdentity | `effb83b40a825d86` · `ELF 64-bit LSB shared object, x86-64, dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2, BuildID[sha1]=e142d25a…, stripped` |
| permissions | root:root 755 |
| status | `available` |
| capabilityProbe | `--version` 正常（版本行 `pandoc 3.11`）；`--list-input-formats` 共 **51** 条，抽样含 `docx`、`html`、`latex`、`markdown`（另有 markdown_github/markdown_mmd/markdown_phpextra/markdown_strict/biblatex）；`--list-output-formats` 共 **76** 条，抽样含 `docx`、`html`/`html4`/`html5`、`latex`、`markdown`（另有 chunkedhtml/biblatex 等） |

## missing 清单

无。两个盘点对象均在位且探针通过。

## anomalies 清单

- `/usr/sbin/soffice` 经多级符号链接最终落在 shell wrapper `/usr/lib/libreoffice/program/soffice`，真实二进制是其同目录的 `soffice.bin` —— Arch 标准布局，非缺陷，但调用方做指纹/权限审计时应以 `soffice.bin` 为准。
- pandoc 的 `file` 类型是 `ELF shared object` 而非 `pie executable`（GHC 产物的常态），非异常，仅记录。
- 无其他异常：两二进制均属 root:root 755、均为官方 pacman 包、版本串与包版本一致（26.8.2.2 ↔ 26.8.2-3；3.11 ↔ 3.11-87）。

## evidence 清单

1. `bash -c "command -v soffice"` → `/usr/sbin/soffice`
2. `timeout 5s readlink -f /usr/sbin/soffice` → `/usr/lib/libreoffice/program/soffice`
3. `timeout 5s soffice --version` → `LibreOffice 26.8.2.2 680(Build:2)`
4. `timeout 5s pacman -Qo /usr/lib/libreoffice/program/soffice.bin` → `/usr/lib/libreoffice/program/soffice.bin 由 libreoffice-fresh 26.8.2-3 所拥有`
5. `timeout 5s ls /usr/lib/libreoffice/program/soffice /usr/lib/libreoffice/program/soffice.bin /usr/lib/libreoffice/program/libswlo.so /usr/lib/libreoffice/program/libsclo.so /usr/lib/libreoffice/program/libsdlo.so /usr/lib/libreoffice/program/libswuilo.so /usr/lib/libreoffice/program/libscuilo.so /usr/lib/libreoffice/program/libsduilo.so` → 8 个路径全部回显存在（无 No such file）
6. `timeout 5s soffice --help | grep -n headless` → `57:   --headless          Starts in "headless mode" which allows using the`
7. `timeout 5s pandoc --version` → `pandoc 3.11` + `Features: +server +lua`
8. `timeout 5s pacman -Qo /usr/bin/pandoc` → `/usr/bin/pandoc 由 pandoc-cli 3.11-87 所拥有`
9. `timeout 5s pandoc --list-input-formats | wc -l` → `51`；`timeout 5s pandoc --list-output-formats | wc -l` → `76`
10. `timeout 5s pandoc --list-input-formats | grep -E 'docx|html|latex|markdown'` → `docx / html / latex / markdown / markdown_github / …`（输出侧同型命中 `docx/html/html4/html5/latex/markdown/…`）
11. `timeout 5s stat -c '%U:%G %a %n' …` → `root:root 755 /usr/lib/libreoffice/program/soffice`、`root:root 755 /usr/lib/libreoffice/program/soffice.bin`、`root:root 755 /usr/bin/pandoc`

## 复现

```bash
bash -c "command -v soffice && readlink -f /usr/sbin/soffice && soffice --version && pacman -Qo /usr/lib/libreoffice/program/soffice.bin && pandoc --version | head -1 && pacman -Qo /usr/bin/pandoc && pandoc --list-input-formats | wc -l && pandoc --list-output-formats | wc -l"
```
预期回显（关键行）：`/usr/sbin/soffice` → `/usr/lib/libreoffice/program/soffice` → `LibreOffice 26.8.2.2 680(Build:2)` → `… 由 libreoffice-fresh 26.8.2-3 所拥有` → `pandoc 3.11` → `… 由 pandoc-cli 3.11-87 所拥有` → `51` → `76`。

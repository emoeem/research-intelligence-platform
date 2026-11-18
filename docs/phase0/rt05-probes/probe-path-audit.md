# RT-0.5 盘点原始取证 · PATH 安全审计（0.5-A 安全面）

- 核查员子代理实测，只读；UID=1000(emo)，附加组 `emo sys network docker libvirt nopasswdlogin rfkill users video storage lp audio wheel flutter`；审计 cwd = `/media/emo/code/pensting/research-platform`；2026-11-18
- PATH 共 26 项，空元素 0 个

## pathDirs 表（路径 | 权限 | 可写? | 受信前缀?）

| # | 路径 | 权限(有效) | 可写? | 受信前缀? | 备注 |
|---|------|-----------|-------|----------|------|
| 1 | /home/emo/.grok/bin | — | — | NO | MISSING |
| 2 | /home/emo/.strix/bin | — | — | NO | MISSING |
| 3 | /home/emo/.local/bin | 755 emo:emo | **可写(emo属主)** | NO | 用户bin |
| 4 | /home/emo/.cargo/bin | 755 emo:emo | **可写(emo属主)** | NO | |
| 5 | /home/emo/.local/share/pnpm/bin | 755 emo:emo | **可写(emo属主)** | NO | |
| 6 | /opt/rocm/bin | 755 root:root | 否 | NO | |
| 7 | /usr/local/sbin | 755 root:root | 否 | NO | |
| 8 | /usr/local/bin | 755 root:root | 否 | **YES** | |
| 9 | /usr/sbin | raw 777 → eff 755 root:root | 否 | **YES** | symlink→bin（即/usr/bin） |
| 10 | /usr/bin | 755 root:root | 否 | **YES** | |
| 11 | /sbin | raw 777 → eff 755 root:root | 否 | **YES** | symlink→usr/bin |
| 12 | /bin | raw 777 → eff 755 root:root | 否 | **YES** | symlink→usr/bin |
| 13 | /tmp/.mount_zcode.0pveme | 755 root:root | 否 | NO | /tmp 前缀（AppImage 挂载，root属主） |
| 14 | /tmp/.mount_zcode.0pveme/usr/sbin | — | — | NO | MISSING |
| 15 | /opt/android-sdk/cmdline-tools/latest/bin | 755 root:root | 否 | NO | |
| 16 | /opt/cuda/bin | 755 root:root | 否 | NO | |
| 17 | /home/emo/.local/share/flatpak/exports/bin | 755 emo:emo | **可写(emo属主)** | NO | flatpak 用户导出 |
| 18 | /var/lib/flatpak/exports/bin | 755 root:root | 否 | NO | |
| 19 | /usr/lib/jvm/default/bin | 755 root:root | 否 | NO | real=java-21-openjdk/bin |
| 20 | /usr/bin/site_perl | 755 root:root | 否 | NO | |
| 21 | /usr/bin/vendor_perl | 755 root:root | 否 | NO | exiftool 实际所在 |
| 22 | /usr/bin/core_perl | 755 root:root | 否 | NO | |
| 23 | /usr/lib/rustup/bin | 755 root:root | 否 | NO | |
| 24 | /tmp/.mount_zcode.0pveme/resources/tools/bfs | 755 root:root | 否 | NO | /tmp 前缀 |
| 25 | /tmp/.mount_zcode.0pveme/resources/tools/ripgrep | 755 root:root | 否 | NO | /tmp 前缀 |
| 26 | /tmp/.mount_zcode.0pveme/resources/tools/ugrep | 755 root:root | 否 | NO | /tmp 前缀 |

**cwd 与 /tmp 检查**：cwd（/media/emo/code/pensting/research-platform）不在 PATH；`/tmp` 本身不在 PATH；但 **5 个 PATH 元素位于 /tmp 前缀下**（/tmp/.mount_zcode.0pveme 及其子目录，root 属主 755，当前用户不可写，test -w = NOT-writable）。空 PATH 元素 0 个。

**可写位判定**：全部 26 项有效权限中，other 位与 group 位均无写位（无 2/3/6/7）；唯一的写位来自 4 个 emo 属主目录的 owner 位（755 → ownerW）。raw=777 的三项为 symlink 自身 link-mode，解引用后均为 755 root:root，`test -w` 实测不可写。

## shadowHits 清单

| 名字 | 命中目录（按 PATH 序） | 判定 |
|------|----------------------|------|
| ffmpeg, ffprobe, tesseract, soffice, pandoc, pdftotext, pdfinfo, pdftoppm, magick, convert, 7z, 7za, 7zr, bsdtar, unzip（共15个） | /usr/sbin : /usr/bin : /sbin : /bin（各4目录，stat：/usr/sbin 777→bin / /usr/bin 755 / /sbin 777→usr/bin / /bin 777→usr/bin） | 按字面准则成立 potential shadow，但 4 目录经 readlink/stat -L 验证为**同一物理目录 /usr/bin 的 symlink 别名**，非真实双副本 |
| exiftool | /usr/bin/vendor_perl（1目录） | 无 shadow |
| mutool | 未命中任何目录 | NOT_FOUND |

真实跨物理目录 shadow：**零命中**。

## candidateTrust 表（名字 | command -v | 受信?）

| 名字 | command -v | 受信? |
|------|-----------|-------|
| ffmpeg | /usr/sbin/ffmpeg | YES |
| ffprobe | /usr/sbin/ffprobe | YES |
| tesseract | /usr/sbin/tesseract | YES |
| soffice | /usr/sbin/soffice | YES |
| pandoc | /usr/sbin/pandoc | YES |
| pdftotext | /usr/sbin/pdftotext | YES |
| pdfinfo | /usr/sbin/pdfinfo | YES |
| pdftoppm | /usr/sbin/pdftoppm | YES |
| mutool | NOT_FOUND | — |
| magick | /usr/sbin/magick | YES |
| convert | /usr/sbin/convert | YES |
| 7z | /usr/sbin/7z | YES |
| 7za | /usr/sbin/7za | YES |
| 7zr | /usr/sbin/7zr | YES |
| bsdtar | /usr/sbin/bsdtar | YES |
| unzip | /usr/sbin/unzip | YES |
| **exiftool** | **/usr/bin/vendor_perl/exiftool** | **NO**（不在受信集合，目录 root 属主 755 不可写） |

不受信名单：`exiftool`（/usr/bin/vendor_perl）；`mutool` 未安装。

## verdict

探测面基本不受 PATH 劫持影响——无 other/group 可写目录、cwd 与 /tmp 未直接入 PATH、候选名无真实同名 shadow（多目录命中均为指向 /usr/bin 的 symlink 别名）；残留面仅两点：4 个 emo 属主可写目录（~/.local/bin、~/.cargo/bin、pnpm、flatpak exports）排在受信目录之前（标准用户 bin 设计，需先持有 emo 身份方可利用），以及 5 个 root 属主的 /tmp/.mount_zcode.0pveme 前缀 PATH 元素（squashfs/AppImage 挂载，当前用户不可写）加 exiftool 解析在受信前缀集合之外（root 属主 755）。

## evidence（命令原文 → 关键回显）

1. `timeout 5s bash -c 'echo "PATH=$PATH"'` → 26 项冒号分隔，含 5 项 /tmp/.mount_zcode.0pveme 前缀、0 个空元素
2. `stat -c "%a %u %U %g %G" -- /home/emo/.local/bin`（审计循环内）→ `755 1000 emo 1000 emo | flags=ownerW | trusted=NO`（~/.cargo/bin、pnpm/bin、flatpak/exports/bin 同型）
3. `ls -ld -- /bin && stat -L -c "%a %U:%G" -- /bin` → `lrwxrwxrwx 1 root root 7 /bin -> usr/bin` + `statL: 755 root:root` + `test-w: NOT-writable`（/usr/sbin→bin、/sbin→usr/bin 同型）
4. `for c in $CANDS; ...; [ -e "$d/$c" ]` → `ffmpeg [4] => /usr/sbin:/usr/bin:/sbin:/bin`（15 个候选同型 4 目录命中）
5. `stat -c "%a %U:%G %N" -- /usr/sbin` → `777 root:root /usr/sbin -> bin | deref=755 root:root`
6. `command -v exiftool` → `/usr/bin/vendor_perl/exiftool | dir=/usr/bin/vendor_perl | trusted=NO`
7. `command -v mutool` → 空（NOT_FOUND）
8. `IFS=":"; for d in $PATH; do [ "$d" = "$(pwd)" ] ...` → `empty_elems=0` + `NONE: cwd/tmp not in PATH`（cwd=/media/emo/code/pensting/research-platform）

## 复现

```bash
timeout 5s bash -c 'IFS=":" read -ra DIRS <<< "$PATH"; for d in "${DIRS[@]}"; do [ -d "$d" ] && stat -c "%a %U:%G %N" -- "$d" || echo "$d MISSING"; done' | head -30
# 预期回显：26 行，/bin /sbin /usr/sbin 显示 lrwxrwxrwx symlink 且 deref 755 root:root，
# /home/emo/.local/bin 等 4 行显示 755 emo:emo；/home/emo/.grok/bin 等 3 行 MISSING

timeout 5s bash -c 'IFS=":" read -ra DIRS <<< "$PATH"; for c in ffmpeg 7z exiftool mutool; do hit=""; for d in "${DIRS[@]}"; do [ -e "$d/$c" ] && hit="$hit $d"; done; echo "$c =>${hit:- NOT_FOUND}"; done'
# 预期回显：ffmpeg => /usr/sbin /usr/bin /sbin /bin；exiftool => /usr/bin/vendor_perl；mutool => NOT_FOUND
```

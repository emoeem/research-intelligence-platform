# wmpf-小程序-cdp-桥接-frida-注入-运行时路由-流量采集-静态分析盲区的解法

> 状态: `notes_only` — 目标/业务特定，通用脚本会失真。
> 本文件记录**真实技术细节与判定框架**，不是脚本。

## 主题

WMPF 微信小程序 CDP 桥接：用 Frida 注入做运行时路由与流量采集


## 为什么需要运行时

静态分析有三个盲区：
1. **代码段加密**（部分小程序用 `wx.security` 混淆）
2. **接口在运行时拼接**（`baseUrl + "/" + module + "/" + action`）
3. **参数动态生成**（签名依赖运行时状态）

这些只能靠运行时抓。

## 两条路径

### 路径 A：WMPF + CDP（推荐，无需 root）

PC 微信的 WMPF（WeChat Mini Program Framework）本质上是一个 Chromium 内核。
开启调试端口后可以用 CDP 直接连：

```bash
# 启动时开调试端口
"C:\\Program Files\\Tencent\\WeChat\\WeChat.exe" --remote-debugging-port=9222

# 或者在已运行时找端口
netstat -ano | findstr LISTENING | findstr "9222 9223 9229"
```

连上后：
```python
import json, websocket
targets = json.loads(requests.get("http://127.0.0.1:9222/json").text)
# 找 type=page 且 url 含 appid 的那个
```

CDP 能做的：
- `Network.enable` → 拿到全部请求（URL、headers、body）
- `Runtime.evaluate` → 在页面上下文执行 JS，读全局变量
- `Debugger.setBreakpointByUrl` → 在签名函数下断点，读参数

### 路径 B：Frida（Android，需 root 或 virtual app）

```javascript
// hook 网络层
Java.perform(function() {
    var URL = Java.use("java.net.URL");
    URL.openConnection.implementation = function() {
        console.log("[URL] " + this.toString());
        return this.openConnection();
    };
});
```

Android 上小程序跑在 `com.tencent.mm:appbrand0` 进程，
需要先 `frida -U -f com.tencent.mm` 再等进程 fork。

## 关键钩子点

| 目标 | 钩子 |
|---|---|
| 请求 URL | `wx.request` / `RequestTask` |
| 请求体 | 同上，看 `data` 参数 |
| 签名函数 | 搜 `sign`、`_t`、`xign` 的赋值点 |
| 路由表 | `getCurrentPages()`、`__wxConfig` |
| 本地存储 | `wx.getStorageSync` |

## 常见坑

- PC 微信重启后端口号会变（不是固定 9222）
- CDP 连上的 target 可能是"渲染进程"，业务逻辑在"逻辑进程"，要两个都连
- Frida 注入微信会触发检测（`frida-server` 进程名、`/data/local/tmp` 下的文件）
- 抓到的流量是明文（小程序不走系统代理），但签名仍然需要还原

## 落地清单

- [ ] 确定目标平台（PC 还是 Android）
- [ ] 开调试端口 / 起 frida-server
- [ ] 抓一次完整业务流程的请求
- [ ] 从请求里找签名参数
- [ ] 在 CDP/Frida 里下断点，读签名输入
- [ ] 离线复现签名


---

## 为什么没有可执行脚本

这类条目的价值在「**针对特定目标的具体契约**」：错误码表、字段名、加密常量、
路由命名。把这些抽象成"通用脚本"，产出的就是一个装满占位符的空壳，
反而比这份说明更容易误导人。

需要自动化时，从中抽出的通用部分已经落到这些脚本里：
- `../crypto_data-加密网关判定法-*/crypto-gateway-probe.py`（判定加密形态）
- `../webpack-spa-前端硬编码-aes-cbc-密钥的定位与复刻-chunk-map-全量下取法/aes-hunter.py`（定位并验证 AES 密钥）
- `../appid-secret-泄露面静态扫描-*/scan-appsecret.py`（批量扫凭证）
- `../webpack-chunk-map-提全量按需-chunk-含-hash-批量抓取/chunk-harvest.py`（抓全量 bundle）

## 待办

- [ ] 若后续对同类目标再遇到一次，把重复出现的部分抽成脚本
- [ ] 每个错误码表补一次实测确认（不同版本会变）

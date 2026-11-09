# POC 库

**实测基线（2026-11-01）**：95 个目录，**15 个有脚本，80 个只有标题**。

原始笔记按主题建了 95 个目录，但脚本只落地了 16%（15/95）。剩下 80 个是「打过的方向」，脚本没写。

## 现状

| 状态 | 数量 | 含义 |
|---|---|---|
| `runnable` | 5 | 参数化完整，改 HOST 就能跑 |
| `needs_target` | 4 | 脚本可用，注释里有真实域名需替换 |
| `partial` | 6 | 能执行但缺 usage 或未完全参数化 |
| `broken` | 0 | — |
| `stub` | 80 | 只有目录名 |
| **实战验证** | **0** | 全部只过了 `bash -n`，没有对真实目标完成判定的记录 |

完整元数据见 `INDEX.json`。

## 用法

```bash
# 先查索引
jq '.entries[] | select(.status=="runnable") | .dir' INDEX.json

# 看某条怎么调
head -6 "pocs/<目录名>/poc.sh"

# 跑（多数是 ./poc.sh <HOST> [附加参数]）
./poc.sh TARGET
```

## 分级标准

`runnable` 必须同时满足三条：**有 usage 注释、参数用 `$1/$2` 而不是硬编码、注释里没有真实域名**。

只过了 `bash -n` 不等于能打。真实判定要落在「对某个目标跑出了可解释的响应差异」上——这个库里目前**一条都没有**，索引里 `verified` 全为 `false` 是准确记录，不是保守。

## 80 个 stub 的主题分布

| 簇 | 数量 | 代表 |
|---|---|---|
| 加密/签名逆向 | 30 | bbin 系包网 entrance-api 加密通道、bkapi AES-128-ECB 反推、Shiro rememberMe key 探测 |
| API/接口枚举 | 22 | entrance-api 错误码差分路由枚举、php-api 控制器三态枚举、hallid 换厅 |
| WAF/风控 | 8 | Log4j 多 header 注入位、SSTI 七级 probe、gin debug/pprof 门禁绕过 |
| 前端产物/JS | 7 | vue-cli sourcemap 还原、appid-secret 静态扫描、hex2bin 站群落地页 |
| 其它 | 9 | GitLab 匿名面体检、TUN 伪 ACK 端口判活、上传点路径可控性四问 |
| 云存储/CDN | 3 | COS 只读审计、CNAME 反查桶名、SNI 分流透明代理绕过 |
| 包网/白牌业务 | 1 | 站群落地页开放重定向 |

**优先级建议**：先补「加密/签名逆向」30 条里的通用部分（Shiro key 表、AES 反推套路、加密通道判定），这些跨项目复用率最高。包网/白牌相关的业务专用逻辑复用率低，最后补。

## 与 nuclei 的关系

`/usr/share/nuclei-templates` 有 **13,687 条**模板，是可用的。这个库补的是 nuclei 覆盖不到的部分：**需要多步交互、需要人工判读响应差异、或者依赖具体业务逻辑**的检测。两者不替代。

## 已知问题

- 80 个 stub 的目录名被截断（长标题），按名字找脚本困难 → 用 `INDEX.json` 检索
- `网易易盾...` 那条原本是笔记误命名为 `poc.sh`，已拆为 `NOTES.md`（原理）+ `poc.sh`（可执行）

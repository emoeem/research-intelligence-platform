> 状态: `script_ok` — 已落地 `poc.sh`（调 `aes-hunter.py`）。
> 本地用真实 key/iv 造样本验证：CBC/ECB 各 1 例命中，错误 key 与随机字节 2 例未命中。

## 已落地脚本

```
./poc.sh --url https://TARGET/          # 抓首页引用的 js
./poc.sh --dir ./chunks                 # 本地 chunk 目录
./poc.sh --file app.js --sample <B64>   # 手动指定密文
```

候选来源四层（有上下文的优先）：显式 key/iv 变量 → `CryptoJS.enc.*.parse()` →
`Buffer.from('...', 'utf8'|'hex')` → 长度精确的裸字面量。

**判定为什么这么严**：解密后必须同时满足 PKCS7 padding 合法、明文可打印 ≥95%、
不含控制字符、命中 JSON/查询串/中文之一。只看「不抛异常」会把随机数据当明文——
第一版就是用全 0 IV 解出的乱码被误判成命中，加了「无控制字符」硬闸才拦掉。

## 实测（2026-11-01）

| 样本 | 期望 | 实际 |
|---|---|---|
| AES-128-CBC，真实 key+iv | HIT | `{"uid":10086,"token":"abc",...}` ✅ |
| AES-128-ECB，同上 key | HIT，mode=ECB | `{"uid":7,"status":1}` ✅ |
| 用 bundle 里不存在的随机 key 加密 | 未命中 | 未命中 ✅ |
| 纯随机 32 字节 | 未命中 | 未命中 ✅ |

## 已知局限

- key 若由运行时派生（`MD5(deviceId)`）或跨 chunk 字符串拼接，本脚本抓不到
- 密文需 16 字节对齐；RSA/GCM 走 `crypto-gateway-probe` 先判形态
- `--url` 只抓首页 `<script src>`，SPA 的动态 import chunk 需先 `--dir`

# webpack spa 前端硬编码 aes cbc 密钥的定位与复刻 chunk map 全量下取法


## 主题

目录名本身记录了这一步的核心结论：**webpack spa 前端硬编码 aes cbc 密钥的定位与复刻 chunk map 全量下取法**

拆出的技术要点：

- `webpack`
- `spa`
- `前端硬编码`
- `aes`
- `cbc`
- `密钥的定位与复刻`
- `chunk`
- `map`
- `全量下取法`

## 方法族: `crypto`

### 执行框架

1. 定位算法边界: 从 bundle / APK / 抓包三处交叉确认「哪个字段是密文」
2. 还原算法: 找 encrypt/decrypt/sign 函数，读它的参数（key/iv/mode/padding）
3. 本地复刻: 用 openssl 或 python 独立实现，先对齐「同一明文 → 同一密文」
4. 验证可写: 用复刻结果构造一个服务端接受的请求（读回 oracle）
5. 确认密钥来源: 硬编码 / 下发接口 / 派生自设备指纹（决定能否批量）

### 常见误报（先排除这些，再下结论）

- 密文长度固定但内容每次不同 → 有随机 IV 或时间戳参与，不是纯 ECB
- 解密出来的明文像二进制 → 前面还有一层压缩（zlib/gzip），先解压再解析
- 本地复刻能加密但服务端拒绝 → 检查是否有 sign 二次校验或时间窗
- 同一请求重放成功 → 说明没有 nonce，可批量重放（也意味着不用逆向）

## 落地清单

- [ ] 补齐可执行脚本 `poc.sh`（参数化 HOST / ENDPOINT）
- [ ] 明确判定阈值（什么结果算命中，什么算未命中）
- [ ] 至少对一个真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 补「阴性对照」用例：构造一个必然不命中的输入，确认判据不会永远返回命中

## 备注

原目录名保留全部信息，未做删改。若后续补脚本，
脚本内的注释应说明「为什么用这个判据」，而不是只写用法。

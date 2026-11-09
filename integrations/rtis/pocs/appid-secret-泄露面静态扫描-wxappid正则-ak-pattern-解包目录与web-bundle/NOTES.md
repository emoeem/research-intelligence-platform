# appid-secret-泄露面静态扫描-wxappid正则-ak-pattern-解包目录与web-bundle

> 状态: `script_ok` — 配套脚本 `scan-appsecret.py`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

AppSecret / AK 的静态扫描难点不在"找"，在"排噪"。一个 webpack 产物目录里
32 位 hex 能出现几千次（内容 hash、模块 id、sourcemap 索引），全报出来等于没报。

## 判据设计

三级分类，只有 L1 可以直接写进报告：
  L1 带键名  `app_secret = "..."` / `secretKey: "..."` —— 几乎无假阳性
  L2 带前缀  AKIA/LTAI/AIza/ghp_/xoxb-/sk_live_ / 完整 JWT / PEM 私钥 —— 厂商前缀本身就说明用途
  L3 裸 hex  32/40/64 位 —— 默认不输出，加 --aggressive 才显示，仅供人工筛

## 常见误报（先排除这些，再下结论）

① md5 与 AppSecret 同形（都是 32 位 hex），无法从形态区分，只能靠键名
② base64 的 32 字符也可能是 24 字节密钥，但多数是普通字符串
③ 微信小程序的 appid 是 `wx` + 16 位 hex，和普通 hash 容易混
④ 光出现 secret 这个词不算，必须紧跟赋值号 + 引号包裹的值

## 验证与收口

必须做有效性验证，否则只是"发现了一个长得像密钥的字符串"：
  AppSecret → curl 'https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid=X&secret=Y'
             返回 access_token = 有效；40125 = 已失效
  云 AK    → 只做 GetCallerIdentity / Describe 类只读调用
  JWT      → 看 alg；HS256 且密钥在包里 = 可签任意 token（这个危害最大）
  数据库   → 只 connect + SELECT 1

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中

# 包网会员站-密码找回-pwdforget-前置判定-vite-包逆出真实契约-beforeenter-守卫判模块是否已下线

> 状态: `notes_only` — 目标/业务特定，通用脚本会失真。
> 本文件记录**真实技术细节与判定框架**，不是脚本。

## 主题

包网会员站密码找回（pwdforget）：用 Vite 包逆出真实契约，判断模块是否已下线


## 问题

密码找回接口 `pwdforget` / `forgetPassword` / `resetPwd` 常见三种状态：

1. **接口活着** —— 可以测短信轰炸、验证码绕过、手机号枚举
2. **接口存在但被 `beforeEnter` 守卫拦住** —— 前端路由守卫里判断某个开关，
   关闭时不渲染页面，但**接口仍然可直连**
3. **后端模块已整套下线** —— 直连返回 404 或统一错误

第 2 种是最常见的误判来源：前端看不到入口 ≠ 后端不可用。

## Vite 包逆向的真实契约

Vite 产物里每个 chunk 是独立的，路由配置通常在 `index.*.js` 的
`__vite__mapDeps` 数组里。找 pwdforget 相关 chunk：

```bash
grep -l 'pwdforget\|forgetPassword\|resetPwd' *.js
```

找到后看三样东西：

1. **请求路径**：`url: "/api/member/pwdforget"` —— 真实路径，可能带前缀
2. **参数字段名**：`{phone, code, newPwd, confirmPwd}` —— 字段名错了会返 400
3. **beforeEnter 守卫**：
```js
beforeEnter: (to, from, next) => {
  if (!store.state.config.enableForgetPwd) return next('/login')
  next()
}
```
这里 `enableForgetPwd` 来自 `/api/config` 或 `/api/site/config` 的返回。

## 三步判定

```
S1 读 /api/config（或 site/config），看 enableForgetPwd 的值
S2 直连 /api/member/pwdforget，看返回什么
S3 组合判断
```

| config 开关 | 直连接口 | 结论 |
|---|---|---|
| true | 200/业务码 | 模块正常，可测业务逻辑 |
| false | **200/业务码** | **前端关了但后端没关** —— 这是最有价值的 |
| false | 404 / 模块下线 | 真的下线了 |
| true | 404 | 配置与代码不一致，可能是灰度 |

## 常见误报

- 直连返回 200 但 body 是"系统维护中" → 模块被业务层禁用，不是可利用
- 返回 400 缺参数 → 接口活着，补上字段再测
- 短信验证码接口是**另一个** URL，别在 pwdforget 上测轰炸

## 落地清单

- [ ] 从 Vite chunk 逆出真实路径与字段名
- [ ] 读 config 接口拿开关值
- [ ] 直连接口，对比开关与结果
- [ ] 若后端未下线，测：验证码复用 / 无频率限制 / 手机号枚举
- [ ] 报告写清"前端已下线但后端仍可直连"，这是最容易被忽略的一类


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

---
name: hunt-clickjacking
description: 点击劫持：X-Frame-Options / CSP frame-ancestors 缺失判定、单帧与多帧 PoC、拖动劫持与双击劫持（防误触绕过）、光标伪装、iframe 覆盖攻击、从 Low 升级到"能触发敏感写操作"的定级口径
whenToUse: "响应缺少 X-Frame-Options 与 CSP frame-ancestors；页面存在删除/转账/授权/关注等一键写操作；需要判断能否跨站诱导点击"
role: vuln-scan
enabled: true
---

# 点击劫持

> 单独看通常只有 Low/Info。**能定级的关键是：被劫持的那个按钮，点了会发生什么。** 删除账号、转账、授权应用、改邮箱、关注——这些才值得写。

## 1. 防护判定（一条命令）

```bash
curl -sD - -o /dev/null "https://$TARGET/settings" \
  | grep -iE "x-frame-options|content-security-policy"
```

| 响应 | 结论 |
|---|---|
| `X-Frame-Options: DENY` | 安全 |
| `X-Frame-Options: SAMEORIGIN` | 安全（同源可嵌，跨站不行） |
| `CSP: frame-ancestors 'none' / 'self'` | 安全 |
| **两者都没有** | **可嵌入** |
| `X-Frame-Options: ALLOW-FROM` | 已废弃，**现代浏览器忽略** → 等于没有 |

## 2. 基础 PoC

```html
<!doctype html>
<style>
  iframe { width: 100vw; height: 100vh; border: 0; opacity: 0.0001; position: relative; z-index: 2; }
  .bait { position: absolute; top: 340px; left: 420px; z-index: 1; font-size: 22px; }
</style>
<div class="bait">点击领取优惠券</div>
<iframe src="https://target.example/settings"></iframe>
```

**关键参数**：
- `opacity: 0.0001`（不是 `0`，`0` 会让某些浏览器不响应事件）
- 位置要用 `top`/`left` 对齐目标按钮的**真实坐标**，不是靠视觉
- iframe 需带 `scrolling="no"` 避免滚动错位

改成 `opacity: 0.3` 生成"取证版"截图（能看见底下是目标站点），再切回 `0.0001` 生成攻击版。

## 3. 防误触绕过

现代浏览器对"跨站 iframe 上的首次点击"会拦截（Chrome 的 `framebusting` 干预）。绕过手法：

| 手法 | 做法 |
|---|---|
| **双击劫持** | 诱导用户"双击"，第一次点击被浏览器吃掉，第二次生效 |
| **拖动劫持** | 诱导用户"拖动滑块"，`mousedown` 在 iframe 上 → `mouseup` 在页面 → 完成拖拽类操作 |
| 多 iframe 叠加 | 多层 iframe，浏览器只拦第一层 |
| 光标伪装 | 在 iframe 上方盖一个假光标图片，隐藏真实光标位置 |

## 4. 配合 XSS 的变体

若能控制同源页面（存储型 XSS），`SAMEORIGIN` 也能被绕：把 payload 放在同源页面里嵌目标页。此时定级随 XSS 走，不单列 clickjacking。

## 5. 定级口径

| 被劫持的按钮 | 定级 |
|---|---|
| 仅跳转/查看 | Info（不写报告） |
| 关注/点赞/收藏 | Low |
| 修改个人资料/邮箱/密码 | Medium |
| 删除内容/转账/支付确认 | **High** |
| 授权第三方应用 / 授予 OAuth scope | **Critical** |

## 6. 常见误判

| 现象 | 真相 |
|---|---|
| 有 `X-Frame-Options: ALLOW-FROM https://x` | 现代浏览器**不支持**，等于无防护 |
| 有 CSP 但只有 `default-src`，没有 `frame-ancestors` | `default-src` **不覆盖** frame-ancestors，仍可嵌 |
| 嵌进去了但白屏 | 目标有 JS framebusting（`top !== self` 跳走）；但 framebusting 可被 `sandbox="allow-scripts allow-same-origin"` 绕 |
| 只测了首页 | 要测**具体操作页**，首页无操作 = 无价值 |

## 纪律

- PoC 页面只嵌自己的登录态，**不实际点他人账号的写操作**
- 报告里必须写清「按钮点下去会发生什么」，只写"可被嵌入"会被判 Info 打回
- 截图需两张：取证版（opacity 0.3，看得出底下是目标站）+ 攻击版（opacity 0.0001）

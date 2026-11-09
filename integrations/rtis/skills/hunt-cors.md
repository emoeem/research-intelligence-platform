---
name: hunt-cors
description: CORS 配置错误挖掘：Origin 反射/前缀后缀匹配/白名单子串/null 源/子域接管四类判定、ACAO+ACAC 组合矩阵、带凭据读取私有接口的 PoC、内网 CORS 横向（of-cors）。区分"仅反射无凭据"（无用）与"反射+凭据"（可读数据）
whenToUse: "目标为前后端分离 API、存在 access-control-allow-origin 头、多子域/多租户部署、需要跨域读私有接口时；或发现 ACAO 回显了请求 Origin"
role: vuln-scan
enabled: true
---

# CORS 配置错误

> **唯一有效的组合**：`Access-Control-Allow-Origin` 反射攻击者域 **且** `Access-Control-Allow-Credentials: true`。
> 只有 ACAO 没有 ACAC → 浏览器不带 Cookie → 读到的是匿名数据 → **不成立**。
> 来源：`knowledge-sources/PayloadsAllTheThings/CORS Misconfiguration/README.md`（274 行）

## 1. 四类错误配置

| 类型 | 请求 Origin | 响应 | 危害 |
|---|---|---|---|
| **Origin 反射** | `https://evil.example` | `ACAO: https://evil.example` + `ACAC: true` | **可直接读任意私有接口** |
| **null 源信任** | `null` | `ACAO: null` + `ACAC: true` | iframe/data-URI 触发，等价反射 |
| **前缀匹配** | `https://target.com.evil.example` | `ACAO: https://target.com.evil.example` | 反射变体 |
| **后缀匹配** | `https://eviltarget.com` | 命中 `endsWith("target.com")` | 注册 `eviltarget.com` 即可 |
| **白名单子串** | `https://evil.example/target.com` | 命中 `includes("target.com")` | 同上 |
| **子域接管** | `https://dead.target.com`（已失效 DNS） | 白名单含 `*.target.com` | 接管子域即获信任 |

## 2. 探测矩阵（5 个 Origin，一次跑完）

```bash
for o in "https://evil.example" "null" "https://$TARGET.evil.example" \
         "https://evil$TARGET" "https://evil.example/$TARGET"; do
  echo "--- Origin: $o"
  curl -s -D - -o /dev/null "https://$TARGET/api/user/profile" \
    -H "Origin: $o" -H "Cookie: $COOKIE" \
    | grep -iE "access-control-allow-(origin|credentials)"
done
```

**判读表**：

| ACAO | ACAC | 结论 |
|---|---|---|
| `*` | 无 | 公开资源，**无所谓**（浏览器禁止 `*` + 凭据） |
| 反射 + `true` | ✅ | **有洞** |
| `null` + `true` | ✅ | **有洞**（data-URI/沙箱 iframe 可打） |
| 固定某域 + `true` | 看该域是否可控/可接管 | 条件成立 |
| 无 ACAO | — | 无洞 |

## 3. PoC（本地自证，不搭外站）

```html
<!-- 存成本地 html，用 file:// 打开 → Origin 为 null -->
<script>
fetch('https://target.example/api/user/profile', {credentials: 'include'})
  .then(r => r.text())
  .then(t => document.title = 'LEN=' + t.length + ' ' + t.slice(0, 80));
</script>
```

`file://` 打开时 Origin 是 `null`，正好命中 null 源配置。若目标只反射具体域，用 host 文件把 `evil.example` 指到 127.0.0.1 再访问。

**判据**：页面能读到响应体（不是 CORS 报错）→ 成立。浏览器 console 会明确打印 `has been blocked by CORS policy`，没有这行就是通了。

## 4. 内网 CORS 横向

`of-cors` 思路：若某内网服务对 Origin 无校验，且可被外部页面访问（同网段/浏览器可达），可跨域读内网面板数据。

```bash
# 探测内网面板的 CORS（需能访问该内网地址）
curl -s -D - -o /dev/null "http://10.0.0.5:8080/api/status" -H "Origin: https://evil.example"
```

## 5. 常见误判

| 现象 | 真实含义 |
|---|---|
| `ACAO: *` + `ACAC: true` | 浏览器**会拒绝**，不是洞（规范禁止） |
| 反射了 Origin 但**只对 OPTIONS 预检** | 实际请求可能不带 ACAO，要测真实 GET/POST |
| 反射了 Origin 但接口本身匿名可读 | 无增量危害，降级 Info |
| WAF 拦截了 `Origin: evil.example` | 换 `Origin: https://target.com.evil.example` 或 `null` |

## 6. 定级

| 场景 | 定级 |
|---|---|
| 反射+凭据，能读**当前用户私有数据**（profile/订单/token） | High |
| 反射+凭据，仅能读非敏感接口 | Medium |
| 仅 `*` 无凭据 | Info（不写） |
| 可读管理接口 / 可写操作被跨域触发 | Critical |

## 纪律

- PoC 只用自己账号的 Cookie，读到自己数据为止
- 不实际把他人数据外带到公网地址；`document.title` 显示长度即可作为证据
- 截图必须包含浏览器 console 无 CORS 报错 + 响应体前若干字节

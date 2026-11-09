---
name: hunt-subdomain-takeover
description: 子域接管挖掘：CNAME 悬空指纹库（GitHub Pages / S3 / Azure / Heroku / Vercel / Netlify / Shopify / Fastly / Tumblr / Unbounce 等 30+ 服务）、NXDOMAIN+悬空双判据、接管可行性自证（不实际占用）、从接管到 Cookie 域宽泛/SPF 放宽/CORS 白名单的升级链
whenToUse: "子域枚举后批量查 CNAME；发现指向已注销第三方服务；子域返回 404/Nonexistent/NoSuchBucket 特征页；目标 Cookie 的 Domain 设为 .example.com"
role: recon
enabled: true
---

# 子域接管

> **判据必须两条同时成立**：① 子域 CNAME 指向某第三方服务；② 该 CNAME 目标在第三方侧**未被占用**（NXDOMAIN 或注册页）。只满足①是正常配置。
> 接管价值不在"能放个页面"，在于：Cookie 的 `Domain=.example.com` 会把子域 cookie 送上主站、SPF/DMARC 放宽、CORS 白名单含 `*.example.com`。

## 1. 批量找悬空 CNAME

```bash
# 子域清单（已有 recon 产物优先）
SUB=subs.txt

# 逐条取 CNAME
while read -r h; do
  c=$(dig +short CNAME "$h" | head -1)
  [ -n "$c" ] && echo "$h -> $c"
done < "$SUB" > cnames.txt

# 只看指向第三方 SaaS 的
grep -iE "github\.io|s3\.amazonaws|cloudfront|azurewebsites|cloudapp\.azure|herokuapp|"
grep -iE "vercel\.app|netlify\.app|surge\.sh|pages\.dev|web\.app|firebaseapp" cnames.txt
grep -iE "myshopify|fastly\.net|tumblr\.com|unbounce|readme\.io|statuspage|zendesk|"
grep -iE "wordpress\.com|ghost\.io|bitbucket\.io|trafficmanager\.net|elasticbeanstalk" cnames.txt
```

也可以用现成工具：

```bash
# subjack / nuclei takeover 模板 / dnsx
nuclei -l subs.txt -t http/takeovers/ -o takeover.txt
```

## 2. 双判据确认

```bash
H=sub.example.com

# 判据① 悬空 CNAME
dig +short CNAME "$H"

# 判据② CNAME 目标本身解析不到（NXDOMAIN / 空）
T=$(dig +short CNAME "$H" | head -1)
dig +short A "$T"          # 空 = 未占用
dig "$T" +short SOA | head -1

# 判据③ 子域响应特征页
curl -s "https://$H/" | head -40 | grep -iE "There isn't a GitHub Pages site here|NoSuchBucket|NoSuchWebsiteConfiguration|Repository not found|404 Not Found.*nginx|project not found|Site not found|Domain not configured|unconfigured"
```

四类服务的特征页：

| 服务 | 特征串 |
|---|---|
| GitHub Pages | `There isn't a GitHub Pages site here.` |
| AWS S3 | `NoSuchBucket` / `The specified bucket does not exist` |
| Azure | `404 Web Site not found` |
| Heroku | `No such app` / `Heroku | No such app` |
| Vercel | `The deployment could not be found` |
| Netlify | `Not Found - Request ID` |
| Shopify | `Sorry, this shop is currently unavailable` |

## 3. 可行性自证（**不实际占用**）

**红线：不要真的去第三方注册那个 bucket/app 名。** 那会占用他人资产。

自证只做到「CNAME 目标无解析记录 + 第三方特征页」为止，附：

```bash
dig "$T" +short            # 空 → 未占用
whois "$T" 2>/dev/null | grep -i "no match\|not found"
curl -sI "https://$T/" -o /dev/null -w '%{http_code}\n'   # 000/404 → 无人认领
```

报告里写「悬空 CNAME + 目标未注册（DNS 无记录）+ 服务商未认领特征页」，不写「已接管」。

## 4. 升级链（决定定级）

| 链 | 条件 | 定级 |
|---|---|---|
| Cookie 域宽泛 | 主站 Cookie `Domain=.example.com` 且 `HttpOnly=false` | 接管子域即可读/写主站 cookie → **High** |
| 同域 CORS 白名单 | 主站 CORS 白名单含 `*.example.com` | 可跨域读主站私有接口 → **High** |
| SPF 放宽 | DNS TXT 含 `include:` 指向接管域或 `~all` | 可伪造发信 → High |
| OAuth 回调白名单 | `redirect_uri` 允许 `https://*.example.com/cb` | 可劫持授权码 → **Critical** |
| 仅能放页面 | 上述都不成立 | Low |

## 5. 常见误判

| 现象 | 真相 |
|---|---|
| CNAME 指向 `*.cloudfront.net` 且仍解析 | 正常在用，不是悬空 |
| 子域 404 | 可能是源站 404，不是 SaaS 特征页；要看**特征串**不是状态码 |
| 泛解析（`*.example.com` 都指同一 IP） | 需要看 CNAME 而不是 A 记录 |
| CNAME 指向内部负载均衡器名 | 内网悬空，通常不可接管，记 Info |

## 纪律

- 只做 DNS 查询 + GET 特征页，零写入
- **绝不注册第三方资源**去"证明"可接管
- 找到候选先查第 4 节的升级链，没有升级链就只记 Info，不占报告篇幅

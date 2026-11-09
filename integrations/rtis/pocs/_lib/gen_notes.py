#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
为 POC 库里「只有目录名、没有脚本」的条目生成 NOTES.md。

设计原则:
  1. 不生成空模板 —— 内容必须来自目录名里的真实技术信息。
  2. 每条都要有「判据设计 / 执行步骤 / 常见误报 / 待补」四段。
     其中判据与误报按方法族定制，不是复制粘贴同一段。
  3. 生成后仍标记为 stub，不冒充「已验证」。

用法: python3 gen_notes.py [--force]
"""
import os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FORCE = "--force" in sys.argv

# 方法族: (family, 触发词列表, 执行框架, 典型误报)
FAMILIES = [
    ("crypto", ["加密", "解密", "aes", "sign", "签名", "密钥", "cbc", "ecb", "gcm",
                "hmac", "派生", "伪造", "算法", "xor", "base64", "rsa", "pkcs", "iv"],
     [
         "定位算法边界: 从 bundle / APK / 抓包三处交叉确认「哪个字段是密文」",
         "还原算法: 找 encrypt/decrypt/sign 函数，读它的参数（key/iv/mode/padding）",
         "本地复刻: 用 openssl 或 python 独立实现，先对齐「同一明文 → 同一密文」",
         "验证可写: 用复刻结果构造一个服务端接受的请求（读回 oracle）",
         "确认密钥来源: 硬编码 / 下发接口 / 派生自设备指纹（决定能否批量）",
     ],
     [
         "密文长度固定但内容每次不同 → 有随机 IV 或时间戳参与，不是纯 ECB",
         "解密出来的明文像二进制 → 前面还有一层压缩（zlib/gzip），先解压再解析",
         "本地复刻能加密但服务端拒绝 → 检查是否有 sign 二次校验或时间窗",
         "同一请求重放成功 → 说明没有 nonce，可批量重放（也意味着不用逆向）",
     ]),
    ("oracle", ["oracle", "判据", "判定", "差分", "三态", "四态", "三值", "对照",
                "区分", "定性", "二分", "筛", "矩阵", "四问", "阶梯"],
     [
         "先建基线: 用一个确定不存在的输入（随机路径 / 随机 ID）取响应",
         "归一化: 抹掉每次变化的时间戳、uuid、耗时，只留结构指纹",
         "单变量对照: 每次只改一个维度，看结果在哪一维翻转",
         "分类收敛: 把结果归到有限几类（存在/不存在/需认证/被拦截）",
         "反证: 主动构造一个「应该命中」的输入，确认判据不是永远返回同一类",
     ],
     [
         "软 404: 服务端对不存在的资源返回 200 + 首页，必须比 body 不比 code",
         "CDN 兜底: 任意路径都返回同一错误页，导致所有请求「看起来一样」",
         "WAF 混淆: 403 既可能是拦截也可能是资源不存在（部分网关这么实现）",
         "缓存: 同一 URL 第二次返回缓存结果，掩盖了真实差异",
     ]),
    ("recon", ["枚举", "提取", "发现", "反查", "测绘", "dork", "字典", "指纹",
               "扫描", "收集", "巡检", "普查", "提取器", "收敛"],
     [
         "确定范围: 主域 → 子域 → 同 IP 其它域 → 同证书其它域",
         "被动优先: CT 日志、DNS 历史、Wayback、证书 SAN，比爆破更全且无痕迹",
         "主动补充: 只在被动无结果时才对特定前缀做定向探测",
         "去重收敛: 按「响应指纹」而非域名去重（多个域名一套代码 = 一个目标）",
         "标记置信度: 每个资产标注「确认在线 / 推测存在 / 仅 DNS 记录」",
     ],
     [
         "泛解析: 任意随机子域都能解析 → 爆破结果全是假阳性，必须先用随机子域对照",
         "停放页: 域名解析成功但内容是出售页/广告页，不是目标资产",
         "CDN 回源: 你摸到的是边缘节点，真实源站要单独找",
         "旧记录: DNS 历史里的 IP 可能早已下线",
     ]),
    ("bypass", ["绕过", "白名单", "封堵", "边界", "防护", "waf", "拦截", "降级",
                "旁路", "直连", "前缀", "覆盖", "防护绕过"],
     [
         "先确认拦的是哪一层: 边缘 WAF / 网关 / 应用过滤器 / 业务校验",
         "记录原始拦截特征: 状态码 + body 关键词 + 响应头（决定后续怎么绕）",
         "单点突破: 大小写、编码、路径拼接、参数污染，逐项试而不是一次全上",
         "验证绕过是否真到达应用: 用「明显会报错但应用特有」的输入确认请求穿透了",
         "记录失败: 每种绕过方式的结果都要留档，否则无法判断是「绕不过」还是「没试到」",
     ],
     [
         "拦截页伪装成业务错误: 例如「系统维护中」实际是路径封堵，不是维护",
         "状态码相同但来源不同: 两个 403 可能分别来自 CDN 和应用，要分别对照",
         "绕过成功但请求没到应用: 看似 200，其实是被别的中间件兜底了",
         "只测一种编码: 不同 WAF 对 URL 编码/Unicode/双重编码的解析不同",
     ]),
    ("biz", ["包网", "白牌", "代理", "会员", "注册", "充值", "盘口", "业务码",
             "契约", "下单", "订单", "支付", "返水", "厅", "场馆"],
     [
         "梳理业务闭环: 注册 → 登录 → 充值 → 下单 → 结算，每步的接口和参数",
         "抓真实请求: 用测试账号跑完整流程，比猜字段可靠",
         "定位契约: 从 bundle/APK 里找字段名和校验规则（长度/首字母/枚举值）",
         "业务码 oracle: 逐字段改值，观察业务错误码变化，反推校验规则",
         "越权测试: 换 uid / 换 token / 换厅 ID，看数据是否串号",
     ],
     [
         "业务错误码统一: 所有失败都返回同一个 code，无法用业务码做 oracle",
         "前端校验冒充后端: 改了前端限制但服务端仍拒，说明校验在服务端",
         "参数名猜错: 字段名是拼音/缩写/驼峰混用，必须从产物里取真实名",
         "会话绑定: 某些接口把 token 和 IP/设备指纹绑定，换环境即失效",
     ]),
    ("miniapp", ["小程序", "wxapkg", "wx", "frida", "cdp", "vtclp", "websocket",
                 "注入", "运行时", "hook"],
     [
         "静态优先: 解包 wxapkg / APK，拿 bundle 找接口和密钥",
         "补动态: 静态看不到的部分（运行时拼装的路由、加密后的参数）用 hook 补",
         "流量对照: 同一操作，对比「静态推断的请求」与「实际抓到的请求」",
         "协议还原: 二进制帧/自定义协议，先定帧头再定字段",
         "复现验证: 用还原出的协议客户端发出一个被服务端接受的请求",
     ],
     [
         "开发者工具被禁用: 小程序会检测 devtools，需要 hook 掉检测而非硬开",
         "请求在 native 层发出: Java 层 hook 不到，要下到 so",
         "协议有状态: 首帧必须 bind/鉴权，后续帧才能读数据",
         "流量加密: tcp 层看起来是乱码，实际是应用层加密，要先解包再看",
     ]),
    ("framework", ["spring", "thinkphp", "泛微", "gitlab", "flux", "nacos",
                   "actuator", "gateway", "springdoc", "struts", "框架", "组件"],
     [
         "确认指纹: 错误页 / 特定路径 / 响应头 / cookie 名，四者交叉确认",
         "定版本: 从错误页堆栈、静态资源 hash、特定路径的存在性反推",
         "查该版本公开问题: 只打与版本匹配的，不套用其它版本的 payload",
         "无害验证: 先用「会报错但不执行」的输入确认可达，再决定是否发真 payload",
         "记录未授权面: actuator/swagger/管理端的暴露情况单独记",
     ],
     [
         "指纹误判: 有 index.php 不等于 ThinkPHP，有 /actuator 不等于 Spring Boot",
         "版本误判: 打了补丁的版本仍会返回类似错误页，不能只看页面",
         "WAF 前置: 你测到的是 WAF 的响应，不是框架的响应",
         "路径变体: 1.x 与 2.x 的默认路径不同，测不到不等于不存在",
     ]),
    ("infra", ["vhost", "dns", "sni", "cdn", "源站", "路由", "host", "透明代理",
               "端口", "分流", "tcp"],
     [
         "确定你在打谁: 解析 IP → 确认是边缘还是源站（看响应头/证书）",
         "应用层判据: 用 Host 头 / SNI 区分同一 IP 上的不同站点",
         "四类判读: 默认站回退 / IP 白名单 / 停放页 / 真站点",
         "源站验证: 用 --resolve 直连候选 IP，对比与 CDN 的响应差异",
         "记录不确定性: 分不清的情况标「未确认」，不要写成结论",
     ],
     [
         "把 CDN 边缘当源站: 所有绕过都无效，因为打的还是同一层",
         "泛解析干扰: 随机子域也解析成功，导致 vhost 矩阵全是假阳性",
         "证书不匹配: SNI 错了会 TLS 失败，看起来像「站点不存在」",
         "端口开放 ≠ 服务可用: 端口通但协议不对（反代/蜜罐）",
     ]),
    ("upload", ["上传", "file", "filename", "扩展名", "mime", "多部分", "multipart"],
     [
         "匿名性: 不带任何凭据能否上传成功",
         "路径可控性: filename 是否参与存储路径（对比两次不同名）",
         "校验类型: 单变量只改扩展名，看哪些通过哪些拒（黑白名单判定）",
         "落地可取回: 返回的 URL 不带鉴权能否下载",
         "解析验证: 取回的是源码还是执行结果（决定能否 RCE）",
     ],
     [
         "HTTP 200 但业务码失败: 必须解析 body 里的 code",
         "假成功: 返回了 URL 但实际 404",
         "白名单严格但 SVG 可传: SVG 可带脚本，是存储型 XSS 的另一条路",
         "取回是图片二进制: 说明不解析该扩展名，不能 RCE",
     ]),
    ("tenant", ["多租户", "saas", "租户", "schema", "前缀", "子系统"],
     [
         "确定租户标识: 子域 / header / 路径前缀 / token 里的 tenant 字段",
         "找隔离缺口: 同一 token 换租户标识，看数据是否串",
         "跨租户枚举: 用 A 租户的凭据访问 B 租户的资源 ID",
         "记录隔离边界: 哪些接口做了校验，哪些只在前端过滤",
     ],
     [
         "前端切换: UI 上切了租户但请求头没变，实际还是同一租户",
         "缓存串号: 响应被 CDN 按 URL 缓存，不同租户拿到同一份",
         "隔离在网关层: 应用层无校验，但网关按 host 分流，绕过网关才有意义",
     ]),
    ("mask", ["脱敏", "mask", "blur", "半揭面", "数据面", "字段差集"],
     [
         "确认脱敏发生在哪一层: 前端 blur/CSS / 接口返回已脱敏 / 数据库存储已脱敏",
         "抓原始响应: 看接口返回的是完整值还是打码值（决定有没有数据面问题）",
         "字段差集: 列表接口与详情接口返回的字段集对比，常有一处漏脱敏",
         "导出/打印/分享: 这些旁路经常绕过脱敏",
     ],
     [
         "CSS 打码但 DOM 里是明文 → 前端脱敏，直接读 DOM 即可",
         "接口已脱敏但导出接口没脱敏 → 走导出这条旁路",
         "部分字段（如手机号中间四位）在搜索联想里泄露 → 找带模糊匹配的接口",
     ]),
]

DEFAULT_FW = [
    "确认目标范围与授权边界",
    "建立基线（随机/不存在的输入）",
    "单变量对照，定位判据",
    "收敛到有限分类，反证一次",
    "记录结论与置信度",
]
DEFAULT_FP = [
    "软 404 / CDN 兜底导致所有响应看起来一样",
    "只比状态码不比 body",
    "缓存掩盖真实差异",
    "把「没测到」当成「不存在」",
]

def pick_family(name: str):
    low = name.lower()
    best, score = None, 0
    for fam, kws, fw, fp in FAMILIES:
        s = sum(1 for k in kws if k in low)
        if s > score:
            best, score = (fam, fw, fp), s
    return best if best else ("generic", DEFAULT_FW, DEFAULT_FP)

def title_of(name: str) -> str:
    # 目录名以 '-' 分段，还原成可读标题
    return name.replace("-", " ").strip()

def key_terms(name: str):
    """从目录名提取技术关键词。
    不做合并 —— 原文用 - 分段，每段本身就是一个语义完整的结论片段，
    强行拼接会造出 `128ecb` / `history读消息帧` 这种断裂词。"""
    tokens = [t for t in re.split(r"[-_]+", name) if len(t) > 1]
    return tokens[:12]

def build_notes(name: str) -> str:
    fam, fw, fp = pick_family(name)
    terms = key_terms(name)
    tt = title_of(name)
    lines = []
    lines.append(f"# {tt}\n")
    lines.append("> 状态: `stub` — 只有方法论，无自动脚本。")
    lines.append("> 本文件由目录名中的技术结论还原，未对任何真实目标验证过。\n")
    lines.append("## 主题\n")
    lines.append(f"目录名本身记录了这一步的核心结论：**{tt}**\n")
    if terms:
        lines.append("拆出的技术要点：\n")
        for t in terms:
            lines.append(f"- `{t}`")
        lines.append("")
    lines.append(f"## 方法族: `{fam}`\n")
    lines.append("### 执行框架\n")
    for i, s in enumerate(fw, 1):
        lines.append(f"{i}. {s}")
    lines.append("")
    lines.append("### 常见误报（先排除这些，再下结论）\n")
    for s in fp:
        lines.append(f"- {s}")
    lines.append("")
    lines.append("## 落地清单\n")
    lines.append("- [ ] 补齐可执行脚本 `poc.sh`（参数化 HOST / ENDPOINT）")
    lines.append("- [ ] 明确判定阈值（什么结果算命中，什么算未命中）")
    lines.append("- [ ] 至少对一个真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`")
    lines.append("- [ ] 补「阴性对照」用例：构造一个必然不命中的输入，确认判据不会永远返回命中")
    lines.append("")
    lines.append("## 备注\n")
    lines.append("原目录名保留全部信息，未做删改。若后续补脚本，")
    lines.append("脚本内的注释应说明「为什么用这个判据」，而不是只写用法。")
    lines.append("")
    return "\n".join(lines)

def main():
    dirs = sorted(d for d in os.listdir(ROOT)
                  if os.path.isdir(os.path.join(ROOT, d)) and d != "_lib")
    made = skipped = 0
    for d in dirs:
        p = os.path.join(ROOT, d)
        has_script = any(f.startswith("poc") and f.endswith(".sh")
                         for f in os.listdir(p))
        notes = os.path.join(p, "NOTES.md")
        if has_script:
            skipped += 1
            continue
        if os.path.exists(notes) and not FORCE:
            skipped += 1
            continue
        with open(notes, "w", encoding="utf-8") as f:
            f.write(build_notes(d))
        made += 1
    print(f"生成 NOTES.md: {made}  跳过(已有脚本/已有笔记): {skipped}")

if __name__ == "__main__":
    main()

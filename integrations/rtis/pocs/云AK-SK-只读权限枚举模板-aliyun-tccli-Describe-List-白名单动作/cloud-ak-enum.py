#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
云 AK/SK 只读权限枚举（白名单动作）

铁律：**只允许 Describe / List / Get 类动作**。任何 Create / Delete / Update /
      Put / Run / Invoke 都在本脚本的拒绝清单里，即使手工传入也不执行。

为什么要有这个约束：
  拿到 AK 后最省事的做法是跑 ScanPermissions 之类的一键枚举，但那些工具会
  发上百个请求、包含写动作，在没有明确授权范围时是不可逆的。本脚本只回答
  一个最小问题：**这组 AK 有效吗？它是哪个身份？它至少能读什么？**

检测逻辑：
  1. 有效性：调用一个零副作用且必定存在的只读接口
       AWS    sts:GetCallerIdentity   （不需要任何权限就能调）
       阿里云  sts:GetCallerIdentity   （同上）
       腾讯云  sts:GetCallerIdentity
  2. 身份：返回的 AccountId / Arn / UserId 说明这是子账号还是主账号
  3. 读取面：按白名单逐个试 Describe*，记录 Allow / Deny / 未知

实现方式：不装 SDK，直接构造签名请求。
  · 阿里云 / 腾讯云 签名复杂（HMAC-SHA1 / TC3），这里走 CLI 是否存在
  · AWS 的 SigV4 用标准库实现，无依赖

用法：
  ./cloud-ak-enum.py <AK> <SK> [--vendor aliyun|tencent|aws] [--region cn-hangzhou]
示例：
  ./cloud-ak-enum.py LTAI5tXXXX **** --vendor aliyun
"""
import base64
import datetime
import hashlib
import hmac
import json
import os
import shutil
import subprocess
import sys
import urllib.parse

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402

# 写动作黑名单。命中即拒绝——这是硬约束，不是提示。
WRITE_VERBS = (
    "create", "delete", "update", "put", "modify", "set", "add", "remove",
    "run", "invoke", "start", "stop", "reboot", "reset", "attach", "detach",
    "bind", "unbind", "grant", "revoke", "associate", "disassociate",
    "import", "export", "copy", "move", "restore", "backup", "apply",
    "enable", "disable", "terminate", "release", "allocate", "assign",
)


def is_read_only(action: str) -> bool:
    low = action.lower()
    if any(v in low for v in WRITE_VERBS):
        return False
    return low.startswith(("describe", "list", "get", "query", "check", "head"))


# 各厂商的白名单只读动作。只保留「一条就能说明身份/规模」的，不做全量枚举。
AWS_READ = [
    ("sts", "GetCallerIdentity", "2011-06-15"),
]
ALIYUN_READ = [
    "DescribeRegions",
    "DescribeInstances",
    "DescribeVpcs",
    "DescribeSecurityGroups",
    "DescribeUsers",
    "ListPolicies",
]
TENCENT_READ = [
    "DescribeRegions",
    "DescribeInstances",
    "DescribeVpcs",
    "DescribeSecurityGroups",
]


# ---- AWS SigV4（标准库实现，无依赖）------------------------------------
def aws_sigv4(ak, sk, service, action, version, region="us-east-1"):
    host = f"{service}.{region}.amazonaws.com"
    endpoint = f"https://{host}/"
    now = datetime.datetime.utcnow()
    amzdate = now.strftime("%Y%m%dT%H%M%SZ")
    datestamp = now.strftime("%Y%m%d")

    body = urllib.parse.urlencode({"Action": action, "Version": version})
    headers = {
        "content-type": "application/x-www-form-urlencoded; charset=utf-8",
        "host": host,
        "x-amz-date": amzdate,
    }
    signed_headers = ";".join(sorted(headers))
    canonical_headers = "".join(f"{k}:{headers[k]}\n" for k in sorted(headers))
    payload_hash = hashlib.sha256(body.encode()).hexdigest()
    canonical_request = "\n".join([
        "POST", "/", "", canonical_headers, signed_headers, payload_hash,
    ])
    scope = f"{datestamp}/{region}/{service}/aws4_request"
    string_to_sign = "\n".join([
        "AWS4-HMAC-SHA256", amzdate, scope,
        hashlib.sha256(canonical_request.encode()).hexdigest(),
    ])

    def _sign(key, msg):
        return hmac.new(key, msg.encode(), hashlib.sha256).digest()

    k = _sign(("AWS4" + sk).encode(), datestamp)
    k = _sign(k, region)
    k = _sign(k, service)
    k = _sign(k, "aws4_request")
    sig = hmac.new(k, string_to_sign.encode(), hashlib.sha256).hexdigest()

    headers["Authorization"] = (
        f"AWS4-HMAC-SHA256 Credential={ak}/{scope}, "
        f"SignedHeaders={signed_headers}, Signature={sig}"
    )
    return C.req(endpoint, method="POST", data=body, headers=headers)


def probe_aws(ak, sk):
    C.hdr("AWS · GetCallerIdentity（零权限要求）")
    r = aws_sigv4(ak, sk, "sts", "GetCallerIdentity", "2011-06-15")
    if not r.ok:
        C.warn(f"请求失败: {r.error or r.status}")
        return None
    if r.status == 200:
        C.ok("AK 有效")
        import re as _re
        for tag in ("Arn", "Account", "UserId"):
            m = _re.search(rf"<{tag}>([^<]+)</{tag}>", r.body)
            if m:
                C.dim(f"  {tag}: {m.group(1)}")
        return True
    if "InvalidClientTokenId" in r.body or "SignatureDoesNotMatch" in r.body:
        C.err("AK/SK 无效（InvalidClientTokenId / SignatureDoesNotMatch）")
        return False
    C.warn(f"未知返回 {r.status}: {r.body[:200]}")
    return None


def probe_cli(vendor, ak, sk):
    """阿里云/腾讯云走官方 CLI（签名太复杂，自己实现容易错，错的签名会给出假阴性）。"""
    if vendor == "aliyun":
        cli = shutil.which("aliyun") or shutil.which("aliyun-cli")
        cmd = [cli, "sts", "GetCallerIdentity"] if cli else None
    else:
        cli = shutil.which("tccli")
        cmd = [cli, "sts", "GetCallerIdentity"] if cli else None

    if not cmd:
        C.warn(f"未安装 {vendor} CLI。签名逻辑不自研（错误的 SigV4/TC3 会产生假阴性）")
        C.dim(f"装法：curl -fsSL https://aliyuncli.alicdn.com/install.sh | bash   # 阿里云")
        C.dim(f"      pip install tccli                                       # 腾讯云")
        return None

    env = dict(os.environ, ALIBABA_CLOUD_ACCESS_KEY_ID=ak, ALIBABA_CLOUD_ACCESS_KEY_SECRET=sk,
               TENCENTCLOUD_SECRET_ID=ak, TENCENTCLOUD_SECRET_KEY=sk)
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=30, env=env)
    except subprocess.TimeoutExpired:
        C.warn("CLI 超时")
        return None
    if p.returncode == 0:
        C.ok("AK 有效")
        C.dim(p.stdout.strip()[:400])
        return True
    C.err(f"CLI 返回 {p.returncode}")
    C.dim((p.stderr or p.stdout).strip()[:300])
    return False


def try_read_actions(vendor, ak, sk, region):
    """逐个试白名单只读动作。只记录 Allow/Deny，不做任何写。"""
    cli = {"aliyun": shutil.which("aliyun"), "tencent": shutil.which("tccli")}.get(vendor)
    if not cli:
        C.dim("无 CLI，跳过读取面枚举")
        return []
    actions = ALIYUN_READ if vendor == "aliyun" else TENCENT_READ
    env = dict(os.environ, ALIBABA_CLOUD_ACCESS_KEY_ID=ak, ALIBABA_CLOUD_ACCESS_KEY_SECRET=sk,
               TENCENTCLOUD_SECRET_ID=ak, TENCENTCLOUD_SECRET_KEY=sk)
    out = []
    for act in actions:
        if not is_read_only(act):
            C.err(f"拒绝执行非只读动作: {act}")
            continue
        svc = "ecs" if "Instance" in act or "Region" in act or "Vpc" in act or "Security" in act else "ram"
        cmd = [cli, svc, act, "--region", region] if vendor == "aliyun" else [cli, svc, act]
        try:
            p = subprocess.run(cmd, capture_output=True, text=True, timeout=20, env=env)
        except subprocess.TimeoutExpired:
            out.append((act, "TIMEOUT"))
            C.dim(f"  {act}: timeout")
            continue
        if p.returncode == 0:
            out.append((act, "ALLOW"))
            C.ok(f"  {act}: ALLOW")
        else:
            err = (p.stderr or p.stdout)
            verdict = "DENY"
            if "Forbidden" in err or "Unauthorized" in err or "not authorized" in err:
                verdict = "DENY"
            elif "InvalidAccessKeyId" in err:
                verdict = "BAD_AK"
            out.append((act, verdict))
            C.dim(f"  {act}: {verdict}")
    return out


def main():
    if len(sys.argv) < 3:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./cloud-ak-enum.py <AK> <SK> [--vendor aws|aliyun|tencent] [--region cn-hangzhou]\n示例：\n  ./cloud-ak-enum.py AKIDxxxx SECRETxxxx --vendor aws")

    ak, sk = sys.argv[1], sys.argv[2]
    vendor = "aws"
    region = "cn-hangzhou"
    if "--vendor" in sys.argv:
        vendor = sys.argv[sys.argv.index("--vendor") + 1]
    if "--region" in sys.argv:
        region = sys.argv[sys.argv.index("--region") + 1]

    C.hdr("0. 约束声明")
    C.dim("本脚本只发 Describe/List/Get 类只读请求")
    C.dim(f"写动作黑名单: {len(WRITE_VERBS)} 个动词，命中即拒绝")
    C.dim(f"厂商: {vendor}")

    C.hdr("1. 有效性 + 身份")
    if vendor == "aws":
        valid = probe_aws(ak, sk)
    else:
        valid = probe_cli(vendor, ak, sk)

    if valid is False:
        C.verdict("cloud-ak-enum", "INVALID", "AK/SK 无效")
        return 1
    if valid is None:
        C.verdict("cloud-ak-enum", "UNKNOWN", "无法判定（缺 CLI 或网络不可达）")
        return 2

    C.hdr("2. 读取面（白名单动作）")
    results = try_read_actions(vendor, ak, sk, region)

    C.hdr("结论")
    allow = [a for a, v in results if v == "ALLOW"]
    C.dim(f"读动作: {len(results)} 试 → {len(allow)} ALLOW")
    if allow:
        C.warn("这组 AK 是有效的，且至少能读以下资源")
        for a in allow:
            C.dim(f"  {a}")
    print("""
报告口径（必须写清，否则会被判"夸大危害"）：
  · 只写「AK 有效 + 能 Describe 到什么」，不写"可控制整个云账号"
  · 能 List 用户/策略 ≠ 能提权，除非你验证了具体的提权路径
  · 主账号 AK 与子账号 AK 的区别必须写：主账号几乎无限制，子账号受限
  · **绝不**执行任何写动作来"证明危害"。拿一条 Describe 的返回就够了

修复建议模板：
  · 立即禁用该 AK，轮换
  · 若来自前端 bundle：改用后端签发临时 STS，有效期 ≤ 1h
  · 若来自代码仓库：清理 git 历史（filter-repo），不只是删文件""")

    C.verdict("cloud-ak-enum", "VALID" if allow else "VALID_LIMITED",
              f"{len(allow)} 只读动作可用")
    return 0


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())

#!/usr/bin/env python3
"""云 AK/SK 只读权限枚举模板（模板，未针对任何真实目标验证）。

判据：拿到 AK/SK 后（来源面：JS/小程序解包/.env/配置中心/GitHub/日志），只调
「Describe / List / Get」白名单动作确认权限边界，绝不写删：
  阿里云（aliyun cli）：
    aliyun sts GetCallerIdentity                                    # 身份与主账号判定
    aliyun ram ListUsers                                            # RAM 用户可列 = 高权限信号
    aliyun ram ListPolicies --policy-type Custom                    # 自定义策略可读
    aliyun oss ls                                                   # OSS 桶列表（ossutil 同判）
    aliyun ecs DescribeInstances --RegionId cn-xxxx --max-items 20  # ECS 可列
    aliyun rds DescribeDBInstances --RegionId cn-xxxx               # RDS 可列
    aliyun kms ListKeys                                             # KMS 可列（可解密数据面信号）
  腾讯云（tccli）：
    tccli sts GetCallerIdentity / tccli cam ListUsers / tccli cam ListPolicies
    tccli cos GET Service（coscli ls）/ tccli cvm DescribeInstances / tccli cdb DescribeDBInstances
  高权限→接管链概念（只判定，不执行）：AliyunRAMFullAccess ⇒ CreateRole/CreatePolicy 自授权；
  OSS 只读 ⇒ 静态数据全量；ECS DescribeInstances ⇒ 翻 metadata 拿实例临时凭据（判定见技能）。

用法：
  export ALIYUN_AK=<ak> ALIYUN_SK=<sk>   # 或 TENCENT_SECRET_ID / TENCENT_SECRET_KEY
  python3 enumerate.py aliyun            # 或 tencent
  输出：每个动作 OK(权限在)/DENIED(权限无)/ERR(配置错) 矩阵 → ak-enum-<ts>.json

依赖：pip install aliyun-python-cli? 实际用官方 CLI：aliyun / tccli（配置 credential 用
  `aliyun configure set --access-key-id ... --access-key-secret ...` 或 -o 参数注入）

纪律：
  - 白名单外动作（PUT/DELETE/Create/Modify）一律不执行；本脚本硬编码白名单，扩展需评审
  - 结果中出现的真实 AK 一律脱敏为 AKID***；报告回填 knowledge.db：category=info-leak/other
  - 生产云环境注意 API 计费与审计告警噪音：Describe 类基本免费，List 类有 QPS 限制，串行执行
"""
import json
import os
import shutil
import subprocess
import sys
import time

ALIYUN_ACTIONS = [
    ("sts", "GetCallerIdentity", []),
    ("ram", "ListUsers", []),
    ("ram", "ListPolicies", ["--policy-type", "Custom"]),
    ("oss", "ls", []),
    ("ecs", "DescribeInstances", ["--RegionId", "cn-hangzhou", "--max-items", "20"]),
    ("rds", "DescribeDBInstances", ["--RegionId", "cn-hangzhou"]),
    ("kms", "ListKeys", []),
]
TENCENT_ACTIONS = [
    ("sts", "GetCallerIdentity", []),
    ("cam", "ListUsers", []),
    ("cam", "ListPolicies", []),
    ("cvm", "DescribeInstances", ["--limit", "20"]),
    ("cdb", "DescribeDBInstances", []),
]


def run(cli, product, action, extra):
    cmd = [cli, product, action] + extra + ["--output", "json"] if cli == "aliyun" else [cli, product, action] + extra
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if r.returncode == 0:
            return "OK", r.stdout[:200]
        return ("DENIED", r.stderr[:200]) if ("Forbidden" in r.stderr or "AccessDenied" in r.stderr or "NoPermission" in r.stderr) else ("ERR", r.stderr[:200])
    except FileNotFoundError:
        return "NOCLI", f"{cli} 未安装"
    except subprocess.TimeoutExpired:
        return "TIMEOUT", ""


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in ("aliyun", "tencent"):
        print(__doc__)
        sys.exit(1)
    vendor = sys.argv[1]
    cli = "aliyun" if vendor == "aliyun" else "tccli"
    if not shutil.which(cli):
        sys.exit(f"未安装 {cli}（阿里: brew/官网下载 aliyun cli；腾讯: pip install tccli）")
    env_ok = any(os.environ.get(k) for k in (("ALIYUN_AK",) if vendor == "aliyun" else ("TENCENT_SECRET_ID",)))
    if not env_ok:
        print("提示: 环境变量未注入 AK，将使用 CLI 当前 profile —— 确认这不是你日常运维的账号！")
        time.sleep(2)
    actions = ALIYUN_ACTIONS if vendor == "aliyun" else TENCENT_ACTIONS
    out = []
    for product, action, extra in actions:
        status, detail = run(cli, product, action, extra)
        out.append({"product": product, "action": action, "status": status, "detail": detail})
        print(f"{product:<6} {action:<20} {status}")
    fn = f"ak-enum-{vendor}-{time.strftime('%Y%m%d-%H%M%S')}.json"
    json.dump({"vendor": vendor, "results": out}, open(fn, "w"), ensure_ascii=False, indent=1)
    print(f"矩阵已存 {fn} —— OK 项即权限边界，人工评估接管链价值后回填")


if __name__ == "__main__":
    main()

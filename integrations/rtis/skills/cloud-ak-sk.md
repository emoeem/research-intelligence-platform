---
name: cloud-ak-sk
description: 云 AK/SK 泄露利用链：泄露来源面、只读白名单判定（阿里 RAM/OSS/ECS 与腾讯 CAM/COS/CVM）、高权限信号识别、接管链概念图、绝不写删纪律
whenToUse: 发现 AccessKey/SecretKey/SecretId 形态凭据时；云上目标的横向前置
role: internal
enabled: true
---

# 云 AK/SK 利用链（只读判定版）

## 一、泄露来源面（哪里捡 AK）

| 来源 | 判定 |
|---|---|
| 前端 JS / 小程序解包 | `LTAI[0-9A-Za-z]{12,28}`（阿里）/ `AKID[0-9A-Za-z]{13,20}`（腾讯）正则（grep 清单见 miniprogram-unpack） |
| .env / 配置文件泄露 | hunt-source-leak 的 .git/.env 扫描命中 |
| 配置中心 | Nacos/Consul 配置明文（衔接 framework-nacos） |
| GitHub/网盘 | org 仓库历史提交（授权范围） |
| 日志/错误页 | STS 临时 token 打印（过期前有效，注意时效） |

## 二、只读判定链（白名单动作，配合知识库枚举模板）

知识库模板 poc `云AK-SK-只读权限枚举模板…/enumerate.py` 一键跑矩阵，手测命令：

```bash
# 阿里云（aliyun cli，AK 经 configure set 或环境注入）
aliyun sts GetCallerIdentity                                   # 身份/主账号类型（RamRole/子账号）
aliyun ram ListUsers                                           # RAM 可列 = 高权限信号
aliyun oss ls                                                  # 桶列表（ossutil 同判）
aliyun ecs DescribeInstances --RegionId cn-hangzhou            # 实例清单
aliyun kms ListKeys                                            # KMS 可列 = 数据加密面可触

# 腾讯云（tccli）
tccli sts GetCallerIdentity && tccli cam ListUsers
tccli cvm DescribeInstances --limit 20 && tccli cdb DescribeDBInstances
```

## 三、高权限信号 → 接管链概念（判定后评估，不执行写操作）

| 信号 | 接管路径概念 |
|---|---|
| `ram:ListUsers` + `ram:CreatePolicy/CreateRole` 权限（试探性 Describe 全通） | AliyunRAMFullAccess 型 → 给自己授权任意权限（自提权），判定到"策略可改"即停 |
| OSS 只读 | 列桶→读静态数据/备份文件（数据库 dump 常在桶里——高危 PII 面） |
| ECS 可列 + 实例挂角色 | 实例 metadata 临时凭据链（需先拿到实例执行面，判定存在性即可） |
| KMS 可列+可解密 | 加密数据的钥匙面（判定 ListKeys 结果即停） |
| 云函数可列+读代码 | 函数源码里常藏下一层凭据（数据库串等） |

## 四、OSS/COS 专项（衔接本地 poc）

桶型 AK 的三段式零凭据枚举、CNAME 反查、只读审计已有 3 条实战 poc（`oss-桶真名反查…`、`腾讯云 COS…` 系列）——AK 判定与桶判定合并出报告：AK 权限边界 × 桶内数据敏感度 = 最终定级。

## 纪律（此技能的铁律）

- **只调 Describe/List/Get**：任何 Create/Update/Delete/Put 一律不做；模板脚本硬编码白名单
- 临时凭据（STS token）有过期时间，判定要快；过期即弃，不尝试续期
- 结果里的真实 AK 全程脱敏（`LTAI****`/`AKID****`）；报告不出现完整凭据
- 回填：category=info-leak（AK 泄露）+ component=阿里云/腾讯云，usage 写"泄露源→判定链→权限边界→影响面"

<!-- 改编自: dafthack/CloudPentestCheatsheets + al0ne/cloud-audit + 官方 CLI 文档 + 本地 AK 枚举模板, 2026-10-31 -->

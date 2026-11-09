# 判别步骤

## 1. 先看域名有几个节点
dig +short HOST A
dig +short HOST CNAME
getent hosts HOST          # 系统解析器结果可能与 dig 不同

## 2. 逐节点测 TCP
for ip in IP1 IP2; do
  timeout 8 bash -c "cat < /dev/null > /dev/tcp/$ip/443" && echo "$ip:443 OPEN" || echo "$ip:443 FILTERED"
done

## 3. 把域名钉死到单个 IP，复测「超时」的路径
curl -sS -o /dev/null -w '%{http_code} %{time_total}\n' \
  --resolve "HOST:443:$LIVE_IP" "https://HOST/超时路径"

## 4. 判定
# 钉死后 404/200 且 time_total < 1s  => DNS 漂移造成的假超时，不是拦截
# 钉死后仍超时                         => 才是路径级问题（继续查 WAF/后端 hang）

# 同一 IP 上换路径对照：若「超时路径」与「正常路径」在活节点上表现一致，
# 则彻底排除路径级拦截。

## 5. 不能得出的结论
- ❌ 「某路径超时 = 该路径被 WAF 拦截 / 有隐藏后台」
- ❌ 「某路径 404 = 全站无此功能」（仍须在每个节点上分别确认）
- ✅ 只能说「在解析到 X 节点时该路径不可达」

# 附：找出哪些解析器返回哪个节点
dig @8.8.8.8 +short HOST A
dig @1.1.1.1 +short HOST A

# 目的
不靠业务码盲猜，直接用「解密站点自己的提交包」拿到写接口的完整参数契约；
再用「零延迟 + 错码对照」判定验证码后端到底绑定没绑定。

# 一、抓站点自己的请求（真实浏览器）
agent-browser open "https://TARGET_HOST:PORT/web/#/joinagent"     # 代理注册前端路由
agent-browser eval "(()=>{window.__L=[];var p=XMLHttpRequest.prototype,o=p.open,s=p.send;
 p.open=function(m,u){this.__m=m;this.__u=u;return o.apply(this,arguments)};
 p.send=function(b){window.__L.push(this.__m+' '+this.__u+' || '+String(b));return s.apply(this,arguments)};return 'hook'})()"
# 用原生 setter 填 React/Vue 受控 input：
#   Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,v)
#   然后 dispatch input/change/blur
agent-browser eval "document.querySelector('input.joinform_submit').click()"
agent-browser dialog accept        # 站点会弹 confirm「是否确定写入?」
agent-browser eval "window.__L[1]" # 取 POST /entrance/api 的 requestData

# 二、解密（key = ASCII(MD5(页面明文 alias))，AES-256-ECB，双层 base64）
python3 ent.py '<requestData>' '<alias 原文，例如 cWloYW5nODg=>'
# 得到 {"url":"/infe/rest/registeragent/addAgent.json","params":"{...}"} → 参数契约完整

# 三、验证码后端绑定判定（关键判据）
#   111032539「验证码输入错误」 = 码进过会话但写错
#   111032540「闲置过久导致验证码失效」 = 会话里根本没有码
# 把 取码→提交 串在同一个 Promise 链里（零延迟），并故意用错码：
agent-browser eval "fetch('/infe/verify/mkcode',{credentials:'include'}).then(r=>r.text())
  .then(t=>{var sr=t.split(';')[1];return fetch('/infe/verify/macpic?SR='+sr,{credentials:'include'})})
  .then(()=>{var b=new URLSearchParams({...契约字段..., rmNum:'zzzz'});
             return fetch('/infe/rest/registeragent/addAgent.json',{method:'POST',credentials:'include',
               headers:{'Content-Type':'application/x-www-form-urlencoded'},body:b})})
  .then(r=>r.text())"
# 零延迟仍 540 ⇒ 排除「TTL 太短 / 读得慢 / OCR 不准」，即该后端不写目标接口的会话。

# 四、判定「页面上到底有没有码可读」
agent-browser eval "JSON.stringify({img:document.querySelectorAll('img').length,canvas:document.querySelectorAll('canvas').length})"
agent-browser eval "JSON.stringify(performance.getEntriesByType('resource').map(r=>r.name).filter(n=>/macpic|mkcode|securimage/.test(n)))"
# img 全是 logo/banner、且验证码请求 0 条 ⇒ 表单本身没有发码端（真人也不可用）。

#!/usr/bin/env node
/* pig4cloud 系白牌包网 加密通道客户端
 * 适用指纹：
 *   - 首页 <html data-template-type=lobby data-render-init=v7.6.2>
 *   - /assets/theme-0/chunk-closure-startup.<hash>.js（1.5MB，obfuscator.io $A 混淆）
 *   - 内联 window.LOBBY_SITE_CONFIG=JSON.parse(decodeURIComponent(atob("...")))
 *   - /cocos/config_data.json 返回 base64 密文
 *   - API: https://<api_domain>/hall/api/*，请求头 x-data-mode: chipher
 *
 * 通道一（OSS 静态配置）：AES-128-ECB / Pkcs7，key = "thanks,pig4cloud"
 *   来源：vj 类构造函数 hj = window.atob("dGhhbmtzLHBpZzRjbG91ZA==")
 * 通道二（/hall API）：AES-128-CBC / Pkcs7(响应) / ZeroPadding(请求)，
 *   iv  = "5421698523412578"
 *   key = md5(staticToken + md5(staticToken)).slice(2,18)
 *   其中 staticToken 明文写在请求头 token: 里（b2e3d672-9d88-47a7-81b4-9d7ffc62054f）
 */
const crypto=require('crypto');
const IV='5421698523412578', S='b2e3d672-9d88-47a7-81b4-9d7ffc62054f';
const md5=s=>crypto.createHash('md5').update(s).digest('hex');
const K=md5(S+md5(S)).slice(2,18);          // 46233ef7d5e864a0

function enc(o){                             // 请求体：ZeroPadding
  let b=Buffer.from(JSON.stringify(o),'utf8');
  const pad=(16-b.length%16)%16; if(pad)b=Buffer.concat([b,Buffer.alloc(pad)]);
  const c=crypto.createCipheriv('aes-128-cbc',Buffer.from(K,'utf8'),Buffer.from(IV,'utf8'));
  c.setAutoPadding(false);
  return Buffer.concat([c.update(b),c.final()]).toString('base64');
}
function dec(t){                             // 响应：Pkcs7，兼容 ZeroPadding
  const d=crypto.createDecipheriv('aes-128-cbc',Buffer.from(K,'utf8'),Buffer.from(IV,'utf8'));
  d.setAutoPadding(false);
  let p=Buffer.concat([d.update(Buffer.from(t.trim(),'base64')),d.final()]);
  const n=p[p.length-1];
  if(n>=1&&n<=16&&p.slice(p.length-n).every(b=>b===n)) p=p.slice(0,p.length-n);
  else { let z=p.length-1; while(z>=0&&p[z]===0)z--; p=p.slice(0,z+1); }
  return p.toString('utf8');
}
function H(){
  return {'x-custom-referer':'https://qt0008.com/','language':'zh','x-request-id':crypto.randomUUID(),
  'timestamp':String(Date.now()),'appsystem':'Android 15','webauthndomain':'qt0008.com','browserfingerid':'',
  'device':crypto.randomUUID(),'appversion':'v7.6.34','physicaldevicemodel':'Google Pixel 9','platformtype':'5',
  'accept':'application/json, text/plain, */*','content-type':'text/plain','x-data-mode':'chipher',
  'token':S,'domain':'qt0008.com','x-version':'7.6.34','browsertype':'Mobile Chrome v131.0.0.0',
  'referer':'https://qt0008.com/','accept-language':'zh',
  'x-object-id':JSON.stringify({uid:'',browserLanguage:'zh-CN',init:{device:'',created:Date.now(),version:1793583493000}}),
  'currency':'USDT','sitecode':'1169','devicebrand':'Google','clienttimezone':'+8','x-device':'2-1',
  'operatingsystem':'Android','devicemodel':'Google Pixel 9','Origin':'https://qt0008.com',
  'user-agent':'Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36'};
}
// 未授权 BFF 聚合代理：一次投递任意多条 /api/* 路由
async function bff(routes,host='www.qitian7.com'){
  const body={scene:'homeFirstLoad',requestId:crypto.randomUUID(),header:null,
    data:routes.map((r,i)=>({id:`sub-req-${i+1}`,route:r.route,params:r.params||{},header:null})),
    time:Math.floor(Date.now()/1000)};
  const r=await fetch(`https://${host}/hall/api/gohal/bffApi`,{method:'POST',headers:H(),body:enc(body)});
  return JSON.parse(dec(await r.text()));
}
module.exports={bff,enc,dec,H,K};

// OSS 配置解密（单独，不需要上面的 token）
// payload = GET https://<site>/cocos/config_data.json
// key = "thanks,pig4cloud"  AES-128-ECB Pkcs7
//   const p=CryptoJS.AES.decrypt(ct, CryptoJS.enc.Utf8.parse("thanks,pig4cloud"),
//              {mode:CryptoJS.mode.ECB,padding:CryptoJS.pad.Pkcs7});

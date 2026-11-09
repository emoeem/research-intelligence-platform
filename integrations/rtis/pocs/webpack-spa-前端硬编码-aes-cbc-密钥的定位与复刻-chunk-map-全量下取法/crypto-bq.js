#!/usr/bin/env node
/* 复刻 bundle 内 bq()：AES-128-CBC + PKCS7，输出 ciphertext 的 Base64（无 Salted__ 头）
 * 用法：node crypto-bq.js enc|dec <value>
 */
const crypto = require('crypto');
const KEY = Buffer.from('CNEBvwi8xYqi8G42', 'utf8');   // 16B ASCII
const IV  = Buffer.from('jCfjQzLrTsfXaR1m', 'utf8');   // 16B ASCII
function enc(pt){const c=crypto.createCipheriv('aes-128-cbc',KEY,IV);
  return Buffer.concat([c.update(pt,'utf8'),c.final()]).toString('base64');}
function dec(b64){const d=crypto.createDecipheriv('aes-128-cbc',KEY,IV);
  return Buffer.concat([d.update(Buffer.from(b64,'base64')),d.final()]).toString('utf8');}
const [m,v]=process.argv.slice(2);
console.log(m==='enc'?enc(v):dec(v));

/* 等价 openssl（用于交叉验证）
   printf '%s' '123456' | openssl enc -aes-128-cbc \
     -K $(printf '%s' 'CNEBvwi8xYqi8G42' | xxd -p) \
     -iv $(printf '%s' 'jCfjQzLrTsfXaR1m' | xxd -p) -base64 -A
   => HtoL18EFMc5Bw6CppwFe7g==
*/

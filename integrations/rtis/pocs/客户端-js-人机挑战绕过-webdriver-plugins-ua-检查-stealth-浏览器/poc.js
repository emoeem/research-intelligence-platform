#!/usr/bin/env node
/* 绕过"客户端 JS 人机挑战"的最小 stealth 浏览器
 *
 * 适用指纹（hudasmvp.com 实测）：
 *   1) 首屏返回 "请稍候… / One moment, please..."，5s 自动 reload
 *   2) 混淆 JS 内含 checks 对象：webdriverCheck / userAgentCheck / appVersionCheck /
 *      pluginArraySpoofing / mimeTypeArraySpoofing / noLanguage / zeroOuterDimensions
 *   3) 通过则构造 form 提交 wsidchk（Q+s 的静态算术）到 /<32位hex路径>
 *   4) 失败则 GET 同一路径并带 failedChecks=<检查名>
 *
 * 关键：挑战只做客户端检查，无服务端行为校验 → 只要 navigator 属性对齐即可通过。
 */
const puppeteer=require('puppeteer-core');
(async()=>{
  const b=await puppeteer.launch({
    executablePath:process.env.CHROME||'/usr/bin/chromium',
    headless:'new',
    args:['--no-sandbox','--disable-dev-shm-usage','--window-size=1366,900','--lang=zh-CN']
  });
  const p=await b.newPage();
  await p.setViewport({width:1366,height:900});
  await p.setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36');
  await p.evaluateOnNewDocument(()=>{
    try{Object.defineProperty(Navigator.prototype,'webdriver',{get:()=>undefined,configurable:true});}catch(e){}
    try{Object.defineProperty(navigator,'languages',{get:()=>['zh-CN','zh','en'],configurable:true});}catch(e){}
    try{Object.defineProperty(navigator,'language',{get:()=>'zh-CN',configurable:true});}catch(e){}
    try{if(window.outerWidth===0)Object.defineProperty(window,'outerWidth',{get:()=>1366});}catch(e){}
    try{if(window.outerHeight===0)Object.defineProperty(window,'outerHeight',{get:()=>900});}catch(e){}
    // 不要动 navigator.plugins / mimeTypes：
    // 检查逻辑是 PluginArray.prototype === navigator.plugins.__proto__ 且 length>0 才判伪；
    // headless 下 length=0 反而通过，替换成 [] 会让 __proto__ 变成 Array.prototype 触发 spoofing。
  });
  await p.goto(process.env.URL||'https://TARGET/',{waitUntil:'networkidle2',timeout:60000});
  await new Promise(r=>setTimeout(r,8000));
  console.log('URL   :',p.url());
  console.log('TITLE :',await p.title());
  console.log('BODY  :',(await p.evaluate(()=>document.body?document.body.innerText.slice(0,300):'')).replace(/\n+/g,' | '));
  await b.close();
})();

/* 判定成功的标志
 *  - URL 跳到真实的 /index 或首页（不再停留在同一路径）
 *  - body 不再是 "请稍候… / 正在验证您的请求"
 *  - 网络面板里不再出现 failedChecks=webdriverCheck 的请求
 */

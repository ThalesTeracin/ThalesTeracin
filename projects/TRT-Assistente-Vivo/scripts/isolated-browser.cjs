'use strict';
// Isolated visible Chrome, supervised only. This process has no access to the personal Chrome profile.
const fs = require('node:fs');
const path = require('node:path');
const {chromium} = require('../app/node_modules/playwright-core');
const ROOT = 'C:\\Centro de Comando TRT-Info-GPT';
const PORTFILE = path.join(ROOT,'data','browser-profile','DevToolsActivePort');
const sleep = ms=>new Promise(resolve=>setTimeout(resolve,ms));
const MAX_RESPONSE = 100000;

function webUrl(value) {
 if(typeof value!=='string'||value.length>2048)throw Error('URL inválida');
 const url=new URL(value);
 if(!['http:','https:'].includes(url.protocol)||url.username||url.password)throw Error('Apenas URLs HTTP(S) sem credenciais são aceitas');
 return url;
}
function currentOrigin(page){
 return webUrl(page.url()).origin;
}
function sameSite(page, expectedUrl){
 const expected=webUrl(expectedUrl);
 if(page.url()!==expected.href)throw Error('A página mudou. Inspecione novamente e conceda nova permissão.');
 return expected.origin;
}
async function browserPort(){
 for(let i=0;i<80;i++){
  if(fs.existsSync(PORTFILE)){
   const n=Number(fs.readFileSync(PORTFILE,'utf8').split(/\r?\n/)[0]);
   if(n>=1024&&n<=65535)return n;
  }
  await sleep(125);
 }
 throw Error('Chrome isolado indisponível');
}
function publicSelector(el){
 const escaped=s=>CSS.escape(s);
 if(el.id) return '#'+escaped(el.id);
 const name=el.getAttribute('name');
 if(name&&document.querySelectorAll(el.tagName.toLowerCase()+'[name="'+name.replaceAll('\\','\\\\').replaceAll('"','\\"')+'"]').length===1)
  return el.tagName.toLowerCase()+'[name="'+name.replaceAll('\\','\\\\').replaceAll('"','\\"')+'"]';
 return '';
}
async function inspectPage(page){
 return page.evaluate(()=>{
  const visible=el=>!!(el.getClientRects().length)&&getComputedStyle(el).visibility!=='hidden';
  const safeText=el=>(el?.innerText||el?.textContent||'').replace(/\s+/g,' ').trim().slice(0,180);
  const selector=el=>{
   if(el.id)return '#'+CSS.escape(el.id);
   const name=el.getAttribute('name');
   if(name){
    const tag=el.tagName.toLowerCase();
    const query=tag+'[name="'+name.replaceAll('\\','\\\\').replaceAll('"','\\"')+'"]';
    if(document.querySelectorAll(query).length===1)return query;
   }
   return '';
  };
  const fields=[...document.querySelectorAll('input,textarea,select')].filter(visible).slice(0,60).map(el=>{
   const form=el.form;
   return {tag:el.tagName.toLowerCase(),type:(el.getAttribute('type')||'text').toLowerCase(),
    id:el.id||'',name:el.getAttribute('name')||'',
    label:(el.labels?.[0]?.innerText||'').replace(/\s+/g,' ').trim().slice(0,90),
    placeholder:(el.getAttribute('placeholder')||'').slice(0,90),
    required:el.required===true,selector:selector(el),
    formId:form?.id||'',autocomplete:(el.getAttribute('autocomplete')||'').slice(0,60),
    options:el.tagName==='SELECT'?[...el.options].slice(0,30).map(o=>({value:o.value,label:safeText(o)})):[]};
  });
  const headings=[...document.querySelectorAll('h1,h2,h3')].slice(0,20).map(safeText).filter(Boolean);
  const links=[...document.querySelectorAll('a[href]')].filter(visible).slice(0,30).map(el=>({
    text:safeText(el).slice(0,100),url:el.href.slice(0,450)
  })).filter(l=>l.text&&/^https?:\/\//.test(l.url));
  const forms=[...document.forms].slice(0,15).map(f=>({
   id:f.id||'',name:f.name||'',
   action:f.action||location.href,method:(f.method||'get').toUpperCase(),
   fields:[...f.elements].filter(el=>el.matches?.('input,select,textarea')).length,
   hasPassword:!!f.querySelector('input[type="password"]'),
   hasCaptcha:!!f.querySelector('[name*="captcha" i],iframe[src*="recaptcha"],iframe[src*="hcaptcha"],[id*="captcha" i]'),
   submitButtons:[...f.querySelectorAll('button[type="submit"],input[type="submit"]')].slice(0,5)
     .map(x=>safeText(x)||x.value||'Enviar')
  }));
  return {fields,headings,links,forms};
 });
}
function blockedField(meta){
 const metaText=[meta.name,meta.id,meta.autocomplete,meta.placeholder,meta.label].join(' ');
 return /password|senha|pass(word)?|otp|2fa|token|secret|cvv|cvc|credit|card|cart[aã]o|cpf|ssn|security.code|pix|bank|banc[oá]ri|captcha|documento|passport|rg\b/i.test(metaText);
}
async function fillItem(page,item,expectedUrl){
 sameSite(page,expectedUrl);
 if(typeof item.selector!=='string'||!item.selector||item.selector.length>180)throw Error('Seleção inválida');
 if(typeof item.value!=='string'||item.value.length>8000)throw Error('Texto inválido');
 const locator=page.locator(item.selector);
 if(await locator.count()!==1)throw Error('O campo deve corresponder a um único elemento: '+item.selector);
 const el=locator.first();
 if(!await el.isVisible()||!await el.isEnabled())throw Error('Campo não está disponível: '+item.selector);
 const meta=await el.evaluate(e=>({
   tag:e.tagName.toLowerCase(),type:(e.getAttribute('type')||'text').toLowerCase(),
   name:e.getAttribute('name')||'',id:e.id||'',
   autocomplete:e.getAttribute('autocomplete')||'',
   placeholder:e.getAttribute('placeholder')||'',
   label:e.labels?.[0]?.innerText||'',
   readonly:e.readOnly===true,
   formAction:e.form?.action||''
 }));
 if(blockedField(meta))throw Error('Campo possivelmente sensível: preenchimento automático bloqueado');
 if(meta.readonly)throw Error('Campo somente leitura');
 if(meta.formAction){
   const target=webUrl(meta.formAction);
   if(target.origin!==currentOrigin(page))throw Error('Formulário envia dados para outro domínio. Preencha manualmente após revisar.');
 }
 if(meta.tag==='select'){
  if(typeof item.value!=='string'||item.value.length>200)throw Error('Seleção inválida');
  await el.selectOption(item.value,{timeout:7000});
 }else if(['input','textarea'].includes(meta.tag)&&['text','search','email','tel','url','number','date'].includes(meta.type)){
  await el.fill(item.value,{timeout:7000});
 }else{
  throw Error('Tipo de campo não autorizado: '+meta.tag+'/'+meta.type);
 }
 // A site can redirect or change page when the user enters data; do not continue.
 sameSite(page,expectedUrl);
}
async function run(){
 const payload=JSON.parse(fs.readFileSync(0,'utf8'));
 const operation=payload.action;
 if(!['inspect','navigate','fill','fill_batch','shutdown'].includes(operation))throw Error('Operação não autorizada');
 const port=await browserPort();
 const browser=await chromium.connectOverCDP('http://127.0.0.1:'+port,{timeout:10000});
 try{
  if(operation==='shutdown'){
   if(payload.confirmed!==true)throw Error('Confirmação exigida para encerrar');
   const session=await browser.newBrowserCDPSession();
   await session.send('Browser.close');
   process.stdout.write(JSON.stringify({ok:true,message:'Chrome isolado encerrado'}));
   return;
  }
  const context=browser.contexts()[0];
  if(!context)throw Error('Perfil Chrome isolado não encontrado');
  const pages=context.pages().filter(p=>!p.isClosed());
  let page=pages.findLast(p=>p.url()!=='about:blank')||pages[pages.length-1];
  if(!page)page=await context.newPage();
  let httpStatus=null;
  if(operation==='navigate'){
   const res=await page.goto(webUrl(payload.url).href,{waitUntil:'domcontentloaded',timeout:20000});
   httpStatus=res?.status()??null;
  }
  if(['fill','fill_batch'].includes(operation)){
   if(payload.confirmed!==true)throw Error('Autorização explícita necessária');
   if(typeof payload.expectedUrl!=='string')throw Error('Inspecione antes de preencher');
   const origin=sameSite(page,payload.expectedUrl);
   if(origin!==payload.allowedOrigin)throw Error('Permissão concedida para outro domínio');
   const rows=operation==='fill'?[{selector:payload.selector,value:payload.value}]:payload.fields;
   if(!Array.isArray(rows)||rows.length<1||rows.length>20)throw Error('Limite de 1 a 20 campos por lote');
   // Verify all selectors, field types and destination URLs before touching any value.
   for(const item of rows){
    if(!item||typeof item.selector!=='string'||typeof item.value!=='string')throw Error('Entrada inválida');
    const count=await page.locator(item.selector).count();
    if(count!==1)throw Error('Seletor inexistente ou ambíguo: '+item.selector);
    const metadata=await page.locator(item.selector).evaluate(e=>({
      tag:e.tagName.toLowerCase(),type:(e.getAttribute('type')||'text').toLowerCase(),
      id:e.id||'',name:e.getAttribute('name')||'',placeholder:e.getAttribute('placeholder')||'',
      label:e.labels?.[0]?.innerText||'',autocomplete:e.getAttribute('autocomplete')||'',
      formAction:e.form?.action||''
    }));
    if(blockedField(metadata))throw Error('Lote contém campo sensível; use preenchimento manual');
    if(metadata.formAction&&webUrl(metadata.formAction).origin!==origin)throw Error('Formulário aponta a domínio externo');
    if(metadata.tag==='select' || (['input','textarea'].includes(metadata.tag)&&['text','search','email','tel','url','number','date'].includes(metadata.type)))continue;
    throw Error('Campo não permitido: '+item.selector);
   }
   for(const item of rows)await fillItem(page,item,payload.expectedUrl);
  }
  const detail=await inspectPage(page);
  const answer={ok:true,action:operation,title:await page.title(),url:page.url(),httpStatus,...detail,
   message:operation==='fill_batch'?'Campos preenchidos com autorização; nenhum formulário foi enviado.':
   operation==='fill'?'Campo preenchido com autorização; nenhum formulário foi enviado.':'Página inspecionada no Chrome isolado.'};
  const output=JSON.stringify(answer);
  if(output.length>MAX_RESPONSE)throw Error('Resultado excessivamente grande');
  process.stdout.write(output);
 }finally{
  try{await browser.close()}catch{}
 }
}
run().catch(error=>{
 const message=error?.message||String(error);
 process.stdout.write(JSON.stringify({ok:false,error:String(message).slice(0,600)}));
 process.exitCode=1;
});

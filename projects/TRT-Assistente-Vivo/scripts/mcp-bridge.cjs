'use strict';
const fs=require('node:fs');
const path=require('node:path');
const {Client}=require('../app/node_modules/@modelcontextprotocol/sdk/dist/cjs/client/index.js');
const {StreamableHTTPClientTransport}=require('../app/node_modules/@modelcontextprotocol/sdk/dist/cjs/client/streamableHttp.js');
const validate=(val)=>{
 const url=new URL(val);
 const host=url.hostname.toLowerCase();
 if(url.protocol!=='http:'|| !['127.0.0.1','localhost','[::1]'].includes(host)
    || url.username || url.password || url.hash)
  throw Error('Apenas servidores MCP HTTP locais sem credenciais embutidas são permitidos');
 return url;
};
async function run(){
 const input=JSON.parse(fs.readFileSync(0,'utf8'));
 const {url,action,tool,args}=input;
 const endpoint=validate(url);
 if(!['list','call'].includes(action))throw Error('Operação MCP desconhecida');
 if(action==='call'&&(!(typeof tool==='string')||tool.length>120||!tool))throw Error('Ferramenta inválida');
 if(args&&JSON.stringify(args).length>16000)throw Error('Argumentos muito extensos');
 const client=new Client({name:'trt-info-gpt',version:'0.5.0'});
 const transport=new StreamableHTTPClientTransport(endpoint);
 try{
  await client.connect(transport);
  const result=action==='list'? await client.listTools():await client.callTool({name:tool,arguments:args||{}});
  if(action==='list'){
   const list=(result.tools||[]).slice(0,60).map(t=>({
      name:t.name,description:String(t.description||'').slice(0,500),
      inputSchema:t.inputSchema
   }));
   process.stdout.write(JSON.stringify({ok:true,tools:list}).slice(0,70000));
  } else {
   const text=(result.content||[]).filter(c=>c.type==='text').map(c=>c.text).join('\n').slice(0,25000);
   process.stdout.write(JSON.stringify({ok:!result.isError,result:text,isError:!!result.isError}));
  }
 } finally { await client.close().catch(()=>{}); }
}
const timer=setTimeout(()=>{process.stdout.write(JSON.stringify({ok:false,error:'Timeout MCP (25s)'}));process.exit(1)},25000);
run().catch(e=>{
 process.stdout.write(JSON.stringify({ok:false,error:String(e.message||e).slice(0,550)}));
 process.exitCode=1;
}).finally(()=>clearTimeout(timer));

'use strict';
const http=require('node:http');
const path=require('node:path');
const fs=require('node:fs');
const os=require('node:os');
const {randomUUID}=require('node:crypto');
const {McpServer}=require('../app/node_modules/@modelcontextprotocol/sdk/dist/cjs/server/mcp.js');
const {StreamableHTTPServerTransport}=require('../app/node_modules/@modelcontextprotocol/sdk/dist/cjs/server/streamableHttp.js');
const {z}=require('../app/node_modules/zod');
const ROOT=path.resolve(__dirname,'..');
const DOCS=path.join(ROOT,'docs');
const PORT=49321;
const sessions=new Map();
function safeDocument(name){
 if(typeof name!=='string'||name.length>130||!name.match(/^[a-zA-Z0-9_.-]+\.(md|txt)$/i)||name.startsWith('.'))throw Error('Nome de arquivo inválido');
 const target=path.join(DOCS,name);
 const real=fs.realpathSync(target);
 const base=fs.realpathSync(DOCS);
 if(path.dirname(real).toLowerCase()!==base.toLowerCase())throw Error('Documento fora da pasta permitida');
 if(!fs.statSync(real).isFile()||fs.statSync(real).size>130000)throw Error('Arquivo indisponível ou muito grande');
 return real;
}
async function session(){
 const server=new McpServer({name:'trt-info-local-tools',version:'0.5.0'});
 server.registerTool('listar_documentos',{title:'Listar documentos do projeto',description:'Lista somente os nomes de documentos .md e .txt no diretório docs do TRT-Info. Não lê arquivos pessoais.',inputSchema:{}},async()=>{
  const names=fs.readdirSync(DOCS,{withFileTypes:true})
    .filter(f=>f.isFile()&&!f.name.startsWith('.')&&/\.(md|txt)$/i.test(f.name))
    .map(f=>f.name).slice(0,90);
  return {content:[{type:'text',text:JSON.stringify(names)}]};
 });
 server.registerTool('ler_documento',{title:'Ler documento autorizado',description:'Lê um arquivo .md ou .txt pelo nome simples na pasta docs, até 30 KB.',inputSchema:{nome:z.string().describe('Nome simples de arquivo, como ENTREGA-V0.4.0.md')}},async({nome})=>{
  try{
   const file=safeDocument(nome);
   const data=fs.readFileSync(file,'utf8').slice(0,30000);
   return {content:[{type:'text',text:data}]};
  }catch(error){return {content:[{type:'text',text:'Erro: '+String(error.message||error)}],isError:true}}
 });
 server.registerTool('status_sistema',{title:'Resumo de memória e arquitetura',description:'Consulta apenas versão da arquitetura, RAM total/livre e uso deste processo. Não retorna dados pessoais.',inputSchema:{}},async()=>{
  const info={architecture:process.arch,platform:process.platform,
    ramTotalMb:Math.round(os.totalmem()/1048576),
    ramFreeMb:Math.round(os.freemem()/1048576),
    mcpProcessMb:Math.round(process.memoryUsage().rss/1048576)};
  return {content:[{type:'text',text:JSON.stringify(info)}]};
 });
 const transport=new StreamableHTTPServerTransport({sessionIdGenerator:randomUUID});
 await server.connect(transport);
 return {server,transport};
}
const web=http.createServer(async(req,res)=>{
 if(req.url!=='/mcp'){res.writeHead(404).end();return}
 const id=req.headers['mcp-session-id'];
 let current=id?sessions.get(id):null;
 if(!current && !id && req.method==='POST') current=await session();
 if(!current){res.writeHead(404).end();return}
 try{
  await current.transport.handleRequest(req,res);
  if(current.transport.sessionId)sessions.set(current.transport.sessionId,current);
 }catch(error){
  if(!res.headersSent)res.writeHead(500);
  res.end();
 }
});
web.listen(PORT,'127.0.0.1',()=>console.log('TRT_MCP_READY'));
async function cleanup(){
 for(const item of sessions.values())await item.server.close().catch(()=>{});
 web.close(()=>process.exit(0));
 setTimeout(()=>process.exit(0),1500).unref();
}
process.on('SIGINT',()=>{void cleanup()});
process.on('SIGTERM',()=>{void cleanup()});

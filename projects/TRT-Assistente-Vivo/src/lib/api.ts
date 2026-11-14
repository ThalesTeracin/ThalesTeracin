import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export interface Provider {
  id: string
  name: string
  baseUrl: string
  model: string
  enabled: boolean
  priority: number
  costStatus: 'free' | 'paid' | 'unknown'
  apiKeySaved?: boolean
}
export interface Session { id: string; title: string; createdAt: string; updatedAt: string }
export interface Message { id: string; sessionId: string; role: 'user'|'assistant'; content: string; provider: string; createdAt: string }
export interface SystemSnapshot {
  memoryTotalMb: number; memoryUsedMb: number; processCount: number; arch: string;
  topProcesses: Array<{name:string;ramMb:number;pid:string}>
}
export interface ChatArgs {
  sessionId: string
  prompt: string
  history: Array<{role:string;content:string}>
  agentPrompt?: string
  providerId?: string
  autoFallback: boolean
  freeOnly: boolean
  browserCommands?: boolean
}
export interface ChatResult { content: string; provider: string; model: string; estimatedTokens: number; switched: boolean }
export interface TaskStep {index:number;kind:string;label:string;status:string;detail:string}
export interface TaskPreview {id:string;expiresInSeconds:number;steps:TaskStep[];notice:string}
export interface TaskReport {id:string;completed:number;success:boolean;steps:TaskStep[];summary:string;containsFormReview:boolean}
export const localTasks = {
  preview:(prompt:string,sessionId:string)=>invoke<TaskPreview>('preview_local_task',{prompt,sessionId}),
  execute:(id:string,sessionId:string)=>invoke<TaskReport>('execute_local_task',{id,sessionId,approved:true}),
  cancel:(id:string)=>invoke<void>('cancel_local_task',{id}),
  progress:(callback:(data:{taskId:string;index:number;label:string;status:string;detail?:string})=>void)=>
    listen<{taskId:string;index:number;label:string;status:string;detail?:string}>('trt:task-progress',event=>callback(event.payload))
}
export const backend = {
  providers: () => invoke<Provider[]>('list_providers'),
  upsertProvider: (provider: Provider, apiKey?: string) => invoke<void>('upsert_provider', {provider, apiKey}),
  deleteProvider: (id: string) => invoke<void>('delete_provider', {id}),
  testProvider: (id: string) => invoke<string>('test_provider', {id}),
  testChat: (id: string) => invoke<string>('test_chat_provider', {id}),
  models: (id: string) => invoke<string[]>('fetch_provider_models', {id}),
  importOpenCode: () => invoke<{imported:number;existing:number;keysAvailable:number;skipped:number;source:string}>('import_from_opencode'),
  sessions: () => invoke<Session[]>('list_sessions'),
  messages: (sessionId: string) => invoke<Message[]>('get_messages', {sessionId}),
  deleteSession: (sessionId: string) => invoke<void>('delete_session', {sessionId}),
  chat: (request: ChatArgs) => invoke<ChatResult>('send_chat', {request}),
  system: () => invoke<SystemSnapshot>('system_snapshot'),
  onChunk: (fn: (payload: {sessionId:string;chunk:string})=>void) => listen<{sessionId:string;chunk:string}>('trt:chunk', e=>fn(e.payload)),
  onSwitch: (fn: (payload: {sessionId:string;provider:string;model:string;reason:string})=>void) => listen<{sessionId:string;provider:string;model:string;reason:string}>('trt:switch',e=>fn(e.payload)),
}
export function errorMessage(error: unknown) {return error instanceof Error ? error.message : String(error)}

export interface ToolInfo { id: string; name: string; installed: boolean; executable: string|null; note: string }
export interface WindowsInspection {
 operatingSystem: string; architecture: string; computer: string; user: string;
 projectExists: boolean; databaseDirectoryExists: boolean; permissions: string; tools: ToolInfo[]
}
export const windowsApi = {
 installedTools: () => invoke<ToolInfo[]>('installed_tools'),
 inspect: () => invoke<WindowsInspection>('inspect_windows'),
 location: (id: string) => invoke<string>('tool_location',{id}),
 openProject: () => invoke<void>('open_project_folder'),
 launchWindows: (id: 'notepad'|'calculator'|'explorer') => invoke<void>('launch_windows_app',{id}),
 launchCli: (id: 'claude'|'opencode') => invoke<void>('launch_cli_terminal',{id}),
}

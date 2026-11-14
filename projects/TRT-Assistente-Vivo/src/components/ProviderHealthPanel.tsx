import { useCallback, useEffect, useRef, useState } from 'react'
import { Activity, Clock3, RefreshCw, ShieldCheck, Wifi, WifiOff } from 'lucide-react'
import { backend, errorMessage, type Provider } from '../lib/api'
import './ProviderHealthPanel.css'

type Status = { state: 'online' | 'offline' | 'checking'; checkedAt: number; latencyMs?: number; detail: string }
const STORE = 'trt-provider-health-v1'
const readCache = (): Record<string, Status> => {
  try { const raw = localStorage.getItem(STORE); return raw ? JSON.parse(raw) as Record<string, Status> : {} } catch { return {} }
}
export default function ProviderHealthPanel({ providers }: { providers: Provider[] }) {
  const [results, setResults] = useState<Record<string, Status>>(readCache)
  const [running, setRunning] = useState(false)
  const [auto, setAuto] = useState(false)
  const mounted = useRef(true)
  const busy = useRef(false)
  const saved = useRef(results)
  const providerRef = useRef(providers)
  useEffect(() => { providerRef.current = providers }, [providers])
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => { saved.current = results; try { localStorage.setItem(STORE, JSON.stringify(results)) } catch { /* storage unavailable */ } }, [results])
  const check = useCallback(async () => {
    if (busy.current) return
    busy.current = true
    if (mounted.current) setRunning(true)
    try {
      // A lightweight GET /models never sends conversation prompts and does not claim to measure quota.
      const items = providerRef.current.filter(p => p.enabled)
      for (const provider of items) {
        if (!mounted.current) break
        setResults(current => ({ ...current, [provider.id]: { state: 'checking', checkedAt: Date.now(), detail: 'Consultando /models' } }))
        const started = performance.now()
        let result: Status
        try {
          const detail = await backend.testProvider(provider.id)
          result = { state: 'online', checkedAt: Date.now(), latencyMs: Math.round(performance.now() - started), detail }
        } catch (error) {
          result = { state: 'offline', checkedAt: Date.now(), latencyMs: Math.round(performance.now() - started), detail: errorMessage(error) }
        }
        if (mounted.current) setResults(current => ({ ...current, [provider.id]: result }))
      }
    } finally {
      busy.current = false
      if (mounted.current) setRunning(false)
    }
  }, [])
  useEffect(() => {
    if (!auto) return
    const timer = window.setInterval(() => { if (document.visibilityState === 'visible') void check() }, 5 * 60 * 1000)
    return () => window.clearInterval(timer)
  }, [auto, check])
  const enabled = providers.filter(p => p.enabled)
  const online = enabled.filter(p => results[p.id]?.state === 'online').length
  const failed = enabled.filter(p => results[p.id]?.state === 'offline').length
  return <section className="panel trt-health" style={{ marginTop: 18 }}>
    <div className="trt-health-heading"><div><h2><Activity size={18} /> Monitor de disponibilidade das APIs</h2><p className="muted">Testa apenas GET /models, com autenticação já salva. Não consome tokens de chat e não confirma a disponibilidade do modelo específico.</p></div>
      <button className="button secondary" disabled={running} onClick={() => void check()}><RefreshCw size={15} className={running ? 'trt-spin' : ''} />{running ? 'Verificando...' : 'Verificar agora'}</button>
    </div>
    <div className="trt-health-summary"><span><Wifi size={16} /> {online} respondendo</span><span><WifiOff size={16} /> {failed} indisponíveis</span><span><Clock3 size={16} /> {enabled.length - online - failed} não verificados</span></div>
    <label className="trt-health-auto"><input type="checkbox" checked={auto} onChange={e => setAuto(e.target.checked)} /> Verificar automaticamente a cada 5 minutos enquanto este painel estiver aberto</label>
    <div className="trt-health-list">{providers.map(p => {
      const r = results[p.id]
      const label = !p.enabled ? 'Desativado' : !r ? 'Não verificado' : r.state === 'online' ? 'API respondeu' : r.state === 'checking' ? 'Verificando' : 'Falha na consulta'
      return <div className="trt-health-row" key={p.id}>
        <div><strong>{p.name}</strong><small>{p.model}</small><small>{p.baseUrl}</small></div>
        <div className="trt-health-result"><span className={'tag ' + (r?.state === 'online' && p.enabled ? 'active' : '')}>{label}</span><small>{r?.checkedAt ? new Date(r.checkedAt).toLocaleString('pt-BR') : 'Sem medições'}</small><small>{r?.latencyMs !== undefined ? r.latencyMs + ' ms · ' : ''}{r?.detail || 'Aguardando teste'}</small></div>
      </div>
    })}</div>
    <div className="info-banner"><ShieldCheck size={18} /><p>HTTP 200 em /models indica apenas que a rota respondeu. Limites, cotas e custos não são inferidos. Erros 401/403 indicam possível autenticação; 429 indica bloqueio ou limite; outros erros podem indicar falta de suporte a /models. Verificações são sequenciais para reduzir carga.</p></div>
  </section>
}


import { useCallback, useEffect, useMemo, useState } from 'react';

// --- Types ---

type AgentState = 'registered' | 'installed' | 'running' | 'stopped' | 'degraded';

interface AgentRecord {
  id: string;
  name: string;
  runtime: string;
  capabilities: string[];
  state: AgentState;
  health: 'healthy' | 'degraded' | 'unhealthy' | 'unknown';
  task_count: number;
  consecutive_health_failures: number;
  last_health_check_unix_ms: number | null;
}

interface FleetStatus {
  total_agents: number;
  running_agents: number;
  degraded_agents: number;
}

interface AuditEvent {
  actor: string;
  action: string;
  target: string;
  timestamp_unix_ms: number;
}

type View = 'fleet' | 'agent-detail' | 'tasks' | 'config' | 'audit';

const POLL_MS = 2_000;

// --- SVGs ---

const Icons = {
  Fleet: () => (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
    </svg>
  ),
  Task: () => (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" />
    </svg>
  ),
  Config: () => (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
    </svg>
  ),
  Audit: () => (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  ),
  Back: () => (
    <svg className="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
    </svg>
  ),
  Check: () => (
    <svg className="w-5 h-5 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
    </svg>
  ),
};

// --- App Component ---

export function App() {
  const [status, setStatus] = useState<FleetStatus>({
    total_agents: 0,
    running_agents: 0,
    degraded_agents: 0,
  });
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [auditEvents, setAuditEvents] = useState<AuditEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>('fleet');
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [wsConnected, setWsConnected] = useState(false);

  // --- WebSocket & Polling ---
  useEffect(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    // WebSocket
    // Note: We use /api/ws because Vite proxies /api -> backend, but standard WS might need explicit proxy config or direct URL.
    // If backend is on same host/port in prod, relative path works.
    // For dev, verify if Vite proxies WS correctly.
    const wsUrl = `${protocol}//${window.location.host}/api/ws`;
    let ws: WebSocket | null = null;

    try {
      ws = new WebSocket(wsUrl);

      ws.onopen = () => setWsConnected(true);
      ws.onclose = () => setWsConnected(false);
      ws.onerror = () => setWsConnected(false);

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          if (data.type === 'fleet_status') {
            setStatus(data.payload);
          } else if (data.type === 'agents') {
            setAgents(data.payload);
          } else if (data.type === 'audit') {
            setAuditEvents(data.payload);
          }
        } catch { /* ignore */ }
      };
    } catch { /* ignore */ }

    return () => ws?.close();
  }, []);

  useEffect(() => {
    let alive = true;
    const refresh = async () => {
      try {
        const [statusRes, agentsRes] = await Promise.all([
          fetch('/api/fleet/status'),
          fetch('/api/agents'),
        ]);

        if (!statusRes.ok || !agentsRes.ok) throw new Error('Failed to fetch dashboard data');

        const [nextStatus, nextAgents] = (await Promise.all([
          statusRes.json(),
          agentsRes.json(),
        ])) as [FleetStatus, AgentRecord[]];

        if (!alive) return;
        setStatus(nextStatus);
        setAgents(nextAgents);
        setError(null);
      } catch (err) {
        if (alive) setError(err instanceof Error ? err.message : 'Unknown error');
      }
    };

    void refresh();
    const timer = setInterval(() => void refresh(), POLL_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (view !== 'audit') return;
    let alive = true;
    const fetchAudit = async () => {
      try {
        const res = await fetch('/api/audit');
        if (res.ok && alive) setAuditEvents(await res.json());
      } catch { /* ignore */ }
    };
    void fetchAudit();
    const timer = setInterval(() => void fetchAudit(), POLL_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [view]);

  const healthyAgents = useMemo(
    () => agents.filter((agent) => agent.health === 'healthy').length,
    [agents],
  );

  const selectedAgent = useMemo(
    () => agents.find((a) => a.id === selectedAgentId) ?? null,
    [agents, selectedAgentId],
  );

  const openAgentDetail = useCallback((id: string) => {
    setSelectedAgentId(id);
    setView('agent-detail');
  }, []);

  // --- Render ---

  return (
    <div className="flex h-screen bg-slate-50 font-sans text-slate-900">
      {/* Sidebar */}
      <aside className="w-64 bg-slate-900 text-slate-100 flex flex-col border-r border-slate-800">
        <div className="p-6">
          <h1 className="text-xl font-bold tracking-tight text-white flex items-center gap-2">
            <div className="w-8 h-8 rounded bg-blue-600 flex items-center justify-center font-black">C</div>
            ClawDen
          </h1>
          <div className="mt-2 flex items-center gap-2 text-xs text-slate-400">
             <div className={`w-2 h-2 rounded-full ${wsConnected ? 'bg-green-500 animate-pulse' : 'bg-slate-500'}`} />
             {wsConnected ? 'Live Connection' : 'Polling Mode'}
          </div>
        </div>

        <nav className="flex-1 px-4 py-4 space-y-1">
          <NavButton 
            active={view === 'fleet' || view === 'agent-detail'} 
            onClick={() => setView('fleet')} 
            icon={<Icons.Fleet />} 
            label="Fleet Overview" 
          />
          <NavButton 
            active={view === 'tasks'} 
            onClick={() => setView('tasks')} 
            icon={<Icons.Task />} 
            label="Task Monitor" 
          />
          <NavButton 
            active={view === 'config'} 
            onClick={() => setView('config')} 
            icon={<Icons.Config />} 
            label="Config Editor" 
          />
          <NavButton 
            active={view === 'audit'} 
            onClick={() => setView('audit')} 
            icon={<Icons.Audit />} 
            label="Audit Log" 
          />
        </nav>

        <div className="p-4 border-t border-slate-800">
          <div className="text-xs text-slate-500">v0.1.0 • @codervisor</div>
          <div className="text-xs text-slate-600 mt-1">ClawDen Runtime</div>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 overflow-auto">
        <header className="bg-white border-b border-slate-200 px-8 py-5 flex items-center justify-between sticky top-0 z-10">
          <h2 className="text-2xl font-semibold text-slate-800">
            {view === 'fleet' && 'Fleet Overview'}
            {view === 'agent-detail' && 'Agent Details'}
            {view === 'tasks' && 'Task Monitor'}
            {view === 'config' && 'Configuration'}
            {view === 'audit' && 'System Audit Log'}
          </h2>
          {error && (
            <div className="bg-red-50 text-red-700 px-4 py-2 rounded-md text-sm border border-red-200 flex items-center animate-pulse">
              <svg className="w-4 h-4 mr-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              {error}
            </div>
          )}
        </header>

        <div className="p-8 max-w-7xl mx-auto space-y-6">
          {view === 'fleet' && (
            <FleetOverview
              status={status}
              agents={agents}
              healthyAgents={healthyAgents}
              onSelectAgent={openAgentDetail}
            />
          )}

          {view === 'agent-detail' && (
            <AgentDetail agent={selectedAgent} onBack={() => setView('fleet')} />
          )}

          {view === 'tasks' && <TaskMonitor agents={agents} />}

          {view === 'config' && <ConfigEditor />}

          {view === 'audit' && <AuditLogViewer events={auditEvents} />}
        </div>
      </main>
    </div>
  );
}

// --- Subcomponents ---

function NavButton({ active, onClick, icon, label }: { active: boolean; onClick: () => void; icon: React.ReactNode; label: string }) {
  return (
    <button
      onClick={onClick}
      className={`w-full flex items-center px-4 py-3 text-sm font-medium rounded-lg transition-colors ${
        active 
          ? 'bg-blue-600 text-white shadow-md' 
          : 'text-slate-400 hover:bg-slate-800 hover:text-white'
      }`}
    >
      <span className="mr-3 opacity-90">{icon}</span>
      {label}
    </button>
  );
}

function FleetOverview({ status, agents, healthyAgents, onSelectAgent }: { status: FleetStatus; agents: AgentRecord[]; healthyAgents: number; onSelectAgent: (id: string) => void }) {
  const getBadgeColor = (agent: AgentRecord) => {
    switch (agent.state) {
      case 'running': return 'bg-green-100 text-green-800';
      case 'degraded': return 'bg-yellow-100 text-yellow-800';
      case 'stopped': return 'bg-gray-100 text-gray-800';
      default: return 'bg-blue-100 text-blue-800';
    }
  };

  const getHealthColor = (health: string) => {
    switch (health) {
      case 'healthy': return 'text-green-600';
      case 'degraded': return 'text-yellow-600';
      case 'unhealthy': return 'text-red-600';
      default: return 'text-gray-400';
    }
  };

  return (
    <>
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        <MetricCard label="Total Agents" value={status.total_agents} subtext="Registered in fleet" color="blue" />
        <MetricCard label="Running" value={status.running_agents} subtext="Active instances" color="green" />
        <MetricCard label="Degraded" value={status.degraded_agents} subtext="Requiring attention" color="yellow" />
        <MetricCard label="Healthy" value={healthyAgents} subtext="Passing checks" color="emerald" />
      </div>

      <div className="bg-white rounded-xl shadow-sm border border-slate-200 overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-100 flex justify-between items-center">
          <h3 className="font-semibold text-slate-800">Active Agents</h3>
          <span className="text-xs font-mono text-slate-400 bg-slate-100 px-2 py-1 rounded">Total: {agents.length}</span>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead className="bg-slate-50 text-slate-500 uppercase tracking-wider text-xs font-semibold">
              <tr>
                <th className="px-6 py-3">Name</th>
                <th className="px-6 py-3">Runtime</th>
                <th className="px-6 py-3">State</th>
                <th className="px-6 py-3">Health</th>
                <th className="px-6 py-3 text-right">Tasks</th>
                <th className="px-6 py-3">Capabilities</th>
                <th className="px-6 py-3 text-right">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {agents.map((agent) => (
                <tr key={agent.id} className="hover:bg-slate-50 transition-colors">
                  <td className="px-6 py-4 font-medium text-slate-900">{agent.name}</td>
                  <td className="px-6 py-4 text-slate-600 font-mono text-xs">{agent.runtime}</td>
                  <td className="px-6 py-4"><StateBadge state={agent.state} /></td>
                  <td className="px-6 py-4"><HealthBadge status={agent.health} /></td>
                  <td className="px-6 py-4 text-right font-mono text-slate-600">{agent.task_count}</td>
                  <td className="px-6 py-4 text-slate-500 max-w-xs truncate">{agent.capabilities.join(', ') || '—'}</td>
                  <td className="px-6 py-4 text-right">
                    <button
                      onClick={() => onSelectAgent(agent.id)}
                      className="text-blue-600 hover:text-blue-800 font-medium text-xs border border-blue-200 hover:border-blue-400 px-3 py-1 rounded transition-colors"
                    >
                      Details
                    </button>
                  </td>
                </tr>
              ))}
              {agents.length === 0 && (
                <tr>
                  <td colSpan={7} className="px-6 py-12 text-center text-slate-400 italic">
                    No agents connected. Waiting for runtime registration...
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}

function AgentDetail({ agent, onBack }: { agent: AgentRecord | null; onBack: () => void }) {
  if (!agent) {
    return (
      <div className="bg-white rounded-xl shadow-sm border border-slate-200 p-12 text-center">
        <h3 className="text-lg font-medium text-slate-900">Agent not found</h3>
        <button onClick={onBack} className="mt-4 text-blue-600 hover:text-blue-800 inline-flex items-center">
          <Icons.Back /> Return to Fleet
        </button>
      </div>
    );
  }

  return (
    <div className="max-w-3xl mx-auto">
      <button onClick={onBack} className="mb-6 text-slate-500 hover:text-slate-900 inline-flex items-center text-sm font-medium transition-colors">
        <Icons.Back /> Back to fleet
      </button>

      <div className="bg-white rounded-xl shadow-sm border border-slate-200 overflow-hidden">
        <div className="px-8 py-6 border-b border-slate-100 flex justify-between items-start bg-slate-50/50">
          <div>
            <h1 className="text-2xl font-bold text-slate-900">{agent.name}</h1>
            <p className="text-slate-500 font-mono text-sm mt-1">{agent.id}</p>
          </div>
          <div className="flex gap-2">
            <StateBadge state={agent.state} />
            <HealthBadge status={agent.health} />
          </div>
        </div>

        <div className="px-8 py-6 space-y-6">
          <DetailRow label="Runtime" value={agent.runtime} mono />
          <DetailRow 
            label="Last Health Check" 
            value={agent.last_health_check_unix_ms 
              ? new Date(agent.last_health_check_unix_ms).toLocaleString() 
              : 'Never'} 
          />
          <DetailRow label="Consecutive Failures" value={agent.consecutive_health_failures.toString()} />
          <div className="border-t border-slate-100 my-4" />
          
          <div>
            <h4 className="text-sm font-semibold text-slate-900 uppercase tracking-wider mb-3">Metrics</h4>
            <div className="grid grid-cols-2 gap-4">
               <div className="bg-slate-50 p-4 rounded-lg border border-slate-100">
                  <div className="text-slate-500 text-xs uppercase">Tasks Completed</div>
                  <div className="text-2xl font-bold text-slate-800 mt-1">{agent.task_count}</div>
               </div>
               {/* Placeholders for future metrics */}
               <div className="bg-slate-50 p-4 rounded-lg border border-slate-100 opacity-50">
                  <div className="text-slate-500 text-xs uppercase">Uptime</div>
                  <div className="text-2xl font-bold text-slate-800 mt-1">—</div>
               </div>
            </div>
          </div>

          <div className="border-t border-slate-100 my-4" />
          
          <div>
             <h4 className="text-sm font-semibold text-slate-900 uppercase tracking-wider mb-3">Capabilities</h4>
             <div className="flex flex-wrap gap-2">
                {agent.capabilities.length > 0 ? (
                  agent.capabilities.map((cap) => (
                    <span key={cap} className="bg-blue-50 text-blue-700 px-3 py-1 rounded-full text-sm border border-blue-100">
                      {cap}
                    </span>
                  ))
                ) : (
                  <span className="text-slate-400 italic">No capabilities advertised</span>
                )}
             </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function TaskMonitor({ agents }: { agents: AgentRecord[] }) {
  const totalTasks = agents.reduce((sum, a) => sum + a.task_count, 0);
  const busiest = [...agents].sort((a, b) => b.task_count - a.task_count);

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <div className="bg-white p-6 rounded-xl shadow-sm border border-slate-200">
           <h3 className="text-sm font-medium text-slate-500 uppercase">Total Tasks Routed</h3>
           <div className="mt-2 text-4xl font-bold text-slate-900">{totalTasks}</div>
        </div>
      </div>

      <div className="bg-white rounded-xl shadow-sm border border-slate-200 overflow-hidden">
        <div className="px-6 py-4 border-b border-slate-100">
          <h3 className="font-semibold text-slate-800">Load Distribution</h3>
        </div>
        <table className="w-full text-left text-sm">
          <thead className="bg-slate-50 text-slate-500 uppercase tracking-wider text-xs font-semibold">
            <tr>
              <th className="px-6 py-3">Agent</th>
              <th className="px-6 py-3">Runtime</th>
              <th className="px-6 py-3 text-right">Tasks</th>
              <th className="px-6 py-3 w-1/3">Load Share</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {busiest.map((agent) => (
              <tr key={agent.id} className="hover:bg-slate-50">
                <td className="px-6 py-4 font-medium text-slate-900">{agent.name}</td>
                <td className="px-6 py-4 text-slate-500 font-mono text-xs">{agent.runtime}</td>
                <td className="px-6 py-4 text-right font-mono text-slate-700">{agent.task_count}</td>
                <td className="px-6 py-4">
                  <div className="w-full bg-slate-100 rounded-full h-2.5 overflow-hidden">
                    <div 
                      className="bg-blue-600 h-2.5 rounded-full" 
                      style={{ width: `${totalTasks > 0 ? (agent.task_count / totalTasks) * 100 : 0}%` }}
                    />
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function ConfigEditor() {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [configText, setConfigText] = useState<string>(
    JSON.stringify(
      {
        agent: {
          name: 'my-agent',
          runtime: 'open-claw',
          model: { provider: 'openai', name: 'gpt-5-mini', api_key_ref: 'secret/openai' },
          tools: [],
          channels: [],
          security: { allowlist: [], sandboxed: true },
        },
      },
      null,
      2,
    ),
  );
  const [parseError, setParseError] = useState<string | null>(null);
  const [deployed, setDeployed] = useState(false);

  const handleValidate = () => {
    try {
      const parsed = JSON.parse(configText);
      if (!parsed.agent?.name) throw new Error('agent.name is required');
      if (!parsed.agent?.model?.provider || !parsed.agent?.model?.name) throw new Error('agent.model.provider and agent.model.name are required');
      setParseError(null);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } catch (e: any) {
      setParseError(e.message || 'Invalid JSON');
    }
  };

  const handleDeploy = () => {
    handleValidate();
    if (!parseError) {
      setDeployed(true);
      setTimeout(() => setDeployed(false), 2000);
    }
  };

  return (
    <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
      <div className="lg:col-span-2 space-y-4">
        <div className="bg-white rounded-xl shadow-sm border border-slate-200 overflow-hidden flex flex-col h-[600px]">
          <div className="bg-slate-100 px-4 py-2 border-b border-slate-200 flex justify-between items-center">
             <span className="text-xs font-mono text-slate-500">config.json</span>
             <span className="text-xs text-slate-400">JSON</span>
          </div>
          <textarea
            value={configText}
            onChange={(e) => {
              setConfigText(e.target.value);
              setParseError(null);
            }}
            className={`flex-1 w-full p-4 font-mono text-sm resize-none focus:outline-none focus:ring-2 focus:ring-blue-500/20 ${
              parseError ? 'bg-red-50 text-red-900' : 'bg-slate-50 text-slate-800'
            }`}
            spellCheck={false}
          />
        </div>
      </div>

      <div className="space-y-6">
        <div className="bg-white p-6 rounded-xl shadow-sm border border-slate-200">
          <h3 className="font-semibold text-slate-900 mb-4">Actions</h3>
          <div className="space-y-3">
             <button onClick={handleValidate} className="w-full py-2 px-4 bg-white border border-slate-300 text-slate-700 font-medium rounded-lg hover:bg-slate-50 transition-colors">
               Validate Syntax
             </button>
             <button onClick={handleDeploy} className="w-full py-2 px-4 bg-blue-600 text-white font-medium rounded-lg hover:bg-blue-700 transition-colors shadow-sm">
               Deploy Configuration
             </button>
          </div>

          {parseError && (
            <div className="mt-6 p-4 bg-red-50 border border-red-100 rounded-lg">
               <div className="text-red-800 text-sm font-semibold mb-1">Validation Error</div>
               <p className="text-red-600 text-xs font-mono">{parseError}</p>
            </div>
          )}

          {deployed && (
            <div className="mt-6 p-4 bg-green-50 border border-green-100 rounded-lg flex items-center">
               <Icons.Check />
               <span className="ml-2 text-green-700 text-sm font-medium">Deployed successfully</span>
            </div>
          )}
        </div>

        <div className="bg-blue-50 p-6 rounded-xl border border-blue-100">
           <h4 className="text-blue-900 font-medium mb-2 text-sm">Help</h4>
           <p className="text-blue-800 text-sm leading-relaxed">
             Edit the canonical ClawDen configuration. This defines agent behaviors, capabilities, and runtime parameters.
             Changes are validated before propagation.
           </p>
        </div>
      </div>
    </div>
  );
}

function AuditLogViewer({ events }: { events: AuditEvent[] }) {
  const sorted = useMemo(
    () => [...events].sort((a, b) => b.timestamp_unix_ms - a.timestamp_unix_ms),
    [events],
  );

  return (
    <div className="bg-white rounded-xl shadow-sm border border-slate-200 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-100 flex justify-between items-center">
        <h3 className="font-semibold text-slate-800">System Activity</h3>
        <span className="text-xs text-slate-500">{sorted.length} events logged</span>
      </div>
      <table className="w-full text-left text-sm">
        <thead className="bg-slate-50 text-slate-500 uppercase tracking-wider text-xs font-semibold">
          <tr>
            <th className="px-6 py-3">Time</th>
            <th className="px-6 py-3">Actor</th>
            <th className="px-6 py-3">Action</th>
            <th className="px-6 py-3">Target</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {sorted.slice(0, 200).map((event, i) => (
            <tr key={i} className="hover:bg-slate-50">
              <td className="px-6 py-4 whitespace-nowrap text-slate-500 text-xs">
                {new Date(event.timestamp_unix_ms).toLocaleString()}
              </td>
              <td className="px-6 py-4 font-medium text-slate-900">{event.actor}</td>
              <td className="px-6 py-4">
                <span className="font-mono text-xs bg-slate-100 px-2 py-1 rounded text-slate-700 border border-slate-200">
                  {event.action}
                </span>
              </td>
              <td className="px-6 py-4 text-slate-600">{event.target}</td>
            </tr>
          ))}
          {sorted.length === 0 && (
            <tr>
              <td colSpan={4} className="px-6 py-12 text-center text-slate-400 italic">
                No audit events recorded yet.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

// --- Helpers ---

function MetricCard({ label, value, subtext, color = "blue" }: { label: string; value: number; subtext?: string; color?: "blue"|"green"|"yellow"|"emerald" }) {
  const colorStyles = {
    blue: "text-blue-600 bg-blue-50 border-blue-100",
    green: "text-green-600 bg-green-50 border-green-100",
    emerald: "text-emerald-600 bg-emerald-50 border-emerald-100",
    yellow: "text-amber-600 bg-amber-50 border-amber-100"
  };

  const style = colorStyles[color];

  return (
    <div className="bg-white rounded-xl shadow-sm border border-slate-200 p-6">
      <div className="text-slate-500 font-medium text-sm mb-1">{label}</div>
      <div className="flex items-end justify-between">
         <div className="text-3xl font-bold text-slate-900">{value}</div>
         <div className={`px-2 py-1 rounded text-xs font-medium ${style}`}>
            {color === 'green' || color === 'emerald' ? '▲' : '•'} Live
         </div>
      </div>
      {subtext && <div className="text-xs text-slate-400 mt-2">{subtext}</div>}
    </div>
  );
}

function StateBadge({ state }: { state: AgentState }) {
   const styles = {
     registered: "bg-slate-100 text-slate-600 border-slate-200",
     installed: "bg-blue-50 text-blue-700 border-blue-200",
     running: "bg-green-50 text-green-700 border-green-200",
     stopped: "bg-slate-100 text-slate-500 border-slate-200",
     degraded: "bg-orange-50 text-orange-700 border-orange-200"
   };

   return (
     <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium border ${styles[state] || styles.registered} capitalize`}>
       {state}
     </span>
   );
}

function HealthBadge({ status }: { status: AgentRecord['health'] }) {
  const styles = {
    healthy: "text-green-600 bg-green-50 border-green-200 dot-green",
    degraded: "text-amber-600 bg-amber-50 border-amber-200 dot-amber",
    unhealthy: "text-red-600 bg-red-50 border-red-200 dot-red",
    unknown: "text-slate-500 bg-slate-50 border-slate-200 dot-slate",
  };
  
  const dotColor = {
    healthy: "bg-green-500",
    degraded: "bg-amber-500",
    unhealthy: "bg-red-500",
    unknown: "bg-slate-400",
  };

  return (
    <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium border ${styles[status]}`}>
      <span className={`w-1.5 h-1.5 rounded-full mr-1.5 ${dotColor[status]}`} />
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}

function DetailRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
   return (
      <div className="flex flex-col sm:flex-row sm:justify-between py-2 border-b border-slate-50 last:border-0 hover:bg-slate-50/50 px-2 -mx-2 rounded">
         <span className="text-sm font-medium text-slate-500">{label}</span>
         <span className={`text-sm text-slate-900 mt-1 sm:mt-0 ${mono ? 'font-mono' : ''}`}>{value}</span>
      </div>
   );
}

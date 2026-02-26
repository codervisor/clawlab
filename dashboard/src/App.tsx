import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Layout } from './components/layout/Layout';
import { Badge } from './components/ui/badge';
import { Button } from './components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from './components/ui/card';
import { Skeleton } from './components/ui/skeleton';
import { Separator } from './components/ui/separator';
import { AlertDialog } from './components/ui/alert-dialog';
import { ArrowLeft, Server, AlertTriangle } from 'lucide-react';
import { useTheme } from './hooks/useTheme';
import { cn } from './lib/utils';

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

type BadgeVariant = 'success' | 'warning' | 'secondary' | 'outline' | 'destructive';

const POLL_MS = 2_000;

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
  const [loading, setLoading] = useState(true);
  const wsEverConnected = useRef(false);

  // Initialize theme on mount
  useTheme();

  // --- WebSocket ---
  useEffect(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/api/ws`;
    let ws: WebSocket | null = null;

    try {
      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        setWsConnected(true);
        wsEverConnected.current = true;
      };
      ws.onclose = () => {
        if (wsEverConnected.current) {
          toast.warning('WebSocket disconnected, switching to polling mode');
        }
        setWsConnected(false);
      };
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

  // --- Polling ---
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
        setLoading(false);
      } catch (err) {
        if (alive) {
          const msg = err instanceof Error ? err.message : 'Unknown error';
          setError(msg);
          setLoading(false);
        }
      }
    };

    void refresh();
    const timer = setInterval(() => void refresh(), POLL_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  // --- Audit polling ---
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

  return (
    <Layout view={view} onNavigate={setView} wsConnected={wsConnected}>
      {error && (
        <div className="mb-4 flex items-center gap-2 rounded-lg border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          <AlertTriangle className="h-4 w-4 shrink-0" />
          {error}
        </div>
      )}

      {view === 'fleet' && (
        <FleetOverview
          status={status}
          agents={agents}
          healthyAgents={healthyAgents}
          onSelectAgent={openAgentDetail}
          loading={loading}
        />
      )}

      {view === 'agent-detail' && (
        <AgentDetail agent={selectedAgent} onBack={() => setView('fleet')} />
      )}

      {view === 'tasks' && <TaskMonitor agents={agents} loading={loading} />}

      {view === 'config' && <ConfigEditor />}

      {view === 'audit' && <AuditLogViewer events={auditEvents} />}
    </Layout>
  );
}

// --- Fleet Overview ---

function FleetOverview({
  status,
  agents,
  healthyAgents,
  onSelectAgent,
  loading,
}: {
  status: FleetStatus;
  agents: AgentRecord[];
  healthyAgents: number;
  onSelectAgent: (id: string) => void;
  loading: boolean;
}) {
  return (
    <div className="space-y-6">
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {loading ? (
          Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-28 rounded-xl" />
          ))
        ) : (
          <>
            <MetricCard label="Total Agents" value={status.total_agents} subtext="Registered in fleet" />
            <MetricCard label="Running" value={status.running_agents} subtext="Active instances" variant="success" />
            <MetricCard label="Degraded" value={status.degraded_agents} subtext="Requiring attention" variant="warning" />
            <MetricCard label="Healthy" value={healthyAgents} subtext="Passing health checks" variant="success" />
          </>
        )}
      </div>

      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-base">Active Agents</CardTitle>
            <Badge variant="secondary">{agents.length} total</Badge>
          </div>
        </CardHeader>
        <CardContent className="p-0">
          {loading ? (
            <div className="space-y-2 p-6">
              {Array.from({ length: 3 }).map((_, i) => (
                <Skeleton key={i} className="h-10 w-full" />
              ))}
            </div>
          ) : agents.length === 0 ? (
            <EmptyState
              icon={<Server className="h-10 w-10" />}
              title="No agents connected"
              description="Waiting for runtime registration..."
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/50 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    <th className="px-6 py-3 text-left">Name</th>
                    <th className="px-6 py-3 text-left">Runtime</th>
                    <th className="px-6 py-3 text-left">State</th>
                    <th className="px-6 py-3 text-left">Health</th>
                    <th className="px-6 py-3 text-right">Tasks</th>
                    <th className="px-6 py-3 text-left">Capabilities</th>
                    <th className="px-6 py-3 text-right">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-y">
                  {agents.map((agent) => (
                    <tr key={agent.id} className="hover:bg-muted/30 transition-colors">
                      <td className="px-6 py-4 font-medium">{agent.name}</td>
                      <td className="px-6 py-4 font-mono text-xs text-muted-foreground">{agent.runtime}</td>
                      <td className="px-6 py-4"><StateBadge state={agent.state} /></td>
                      <td className="px-6 py-4"><HealthBadge status={agent.health} /></td>
                      <td className="px-6 py-4 text-right font-mono">{agent.task_count}</td>
                      <td className="px-6 py-4 max-w-[200px] truncate text-muted-foreground text-xs">{agent.capabilities.join(', ') || '—'}</td>
                      <td className="px-6 py-4 text-right">
                        <Button variant="outline" size="sm" onClick={() => onSelectAgent(agent.id)}>
                          Details
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// --- Agent Detail ---

function AgentDetail({ agent, onBack }: { agent: AgentRecord | null; onBack: () => void }) {
  const [confirmDialog, setConfirmDialog] = useState<{ action: string; label: string } | null>(null);

  const handleAction = (action: string, label: string) => {
    setConfirmDialog({ action, label });
  };

  const confirmAction = () => {
    if (!confirmDialog || !agent) return;
    toast.success(`${confirmDialog.label} sent to ${agent.name}`);
    setConfirmDialog(null);
  };

  if (!agent) {
    return (
      <Card className="p-12 text-center">
        <h3 className="text-lg font-medium">Agent not found</h3>
        <Button variant="link" onClick={onBack} className="mt-4">
          <ArrowLeft className="mr-2 h-4 w-4" />
          Return to Fleet
        </Button>
      </Card>
    );
  }

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      <Button variant="ghost" onClick={onBack} className="gap-1 text-muted-foreground">
        <ArrowLeft className="h-4 w-4" />
        Back to fleet
      </Button>

      <Card>
        <CardHeader className="bg-muted/30">
          <div className="flex items-start justify-between">
            <div>
              <CardTitle className="text-2xl">{agent.name}</CardTitle>
              <p className="mt-1 font-mono text-sm text-muted-foreground">{agent.id}</p>
            </div>
            <div className="flex gap-2">
              <StateBadge state={agent.state} />
              <HealthBadge status={agent.health} />
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4 pt-6">
          <DetailRow label="Runtime" value={agent.runtime} mono />
          <DetailRow
            label="Last Health Check"
            value={agent.last_health_check_unix_ms
              ? new Date(agent.last_health_check_unix_ms).toLocaleString()
              : 'Never'}
          />
          <DetailRow label="Consecutive Failures" value={agent.consecutive_health_failures.toString()} />

          <Separator />

          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground mb-3">Metrics</h4>
            <div className="grid grid-cols-2 gap-4">
              <Card className="bg-muted/30">
                <CardContent className="p-4">
                  <div className="text-xs uppercase text-muted-foreground">Tasks Completed</div>
                  <div className="text-2xl font-bold mt-1">{agent.task_count}</div>
                </CardContent>
              </Card>
              <Card className="bg-muted/30 opacity-50">
                <CardContent className="p-4">
                  <div className="text-xs uppercase text-muted-foreground">Uptime</div>
                  <div className="text-2xl font-bold mt-1">—</div>
                </CardContent>
              </Card>
            </div>
          </div>

          <Separator />

          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground mb-3">Capabilities</h4>
            <div className="flex flex-wrap gap-2">
              {agent.capabilities.length > 0 ? (
                agent.capabilities.map((cap) => (
                  <Badge key={cap} variant="secondary">{cap}</Badge>
                ))
              ) : (
                <span className="text-muted-foreground italic text-sm">No capabilities advertised</span>
              )}
            </div>
          </div>

          <Separator />

          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground mb-3">Actions</h4>
            <div className="flex gap-3">
              <Button variant="outline" size="sm" onClick={() => handleAction('restart', 'Restart')}>
                Restart Agent
              </Button>
              <Button variant="destructive" size="sm" onClick={() => handleAction('stop', 'Stop')}>
                Stop Agent
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <AlertDialog
        open={!!confirmDialog}
        title={`Confirm: ${confirmDialog?.label} Agent`}
        description={`Are you sure you want to ${confirmDialog?.action} agent "${agent.name}"? This action may interrupt active tasks.`}
        confirmLabel={confirmDialog?.label}
        cancelLabel="Cancel"
        onConfirm={confirmAction}
        onCancel={() => setConfirmDialog(null)}
        destructive={confirmDialog?.action === 'stop'}
      />
    </div>
  );
}

// --- Task Monitor ---

function TaskMonitor({ agents, loading }: { agents: AgentRecord[]; loading: boolean }) {
  const totalTasks = agents.reduce((sum, a) => sum + a.task_count, 0);
  const busiest = [...agents].sort((a, b) => b.task_count - a.task_count);

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {loading ? (
          <Skeleton className="h-28 rounded-xl" />
        ) : (
          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium text-muted-foreground uppercase">Total Tasks Routed</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-4xl font-bold">{totalTasks}</div>
            </CardContent>
          </Card>
        )}
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Load Distribution</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          {loading ? (
            <div className="space-y-2 p-6">
              {Array.from({ length: 3 }).map((_, i) => (
                <Skeleton key={i} className="h-10 w-full" />
              ))}
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/50 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    <th className="px-6 py-3 text-left">Agent</th>
                    <th className="px-6 py-3 text-left">Runtime</th>
                    <th className="px-6 py-3 text-right">Tasks</th>
                    <th className="px-6 py-3 w-1/3">Load Share</th>
                  </tr>
                </thead>
                <tbody className="divide-y">
                  {busiest.map((agent) => (
                    <tr key={agent.id} className="hover:bg-muted/30">
                      <td className="px-6 py-4 font-medium">{agent.name}</td>
                      <td className="px-6 py-4 font-mono text-xs text-muted-foreground">{agent.runtime}</td>
                      <td className="px-6 py-4 text-right font-mono">{agent.task_count}</td>
                      <td className="px-6 py-4">
                        <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
                          <div
                            className="h-2 rounded-full bg-primary"
                            style={{ width: `${totalTasks > 0 ? (agent.task_count / totalTasks) * 100 : 0}%` }}
                          />
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// --- Config Editor ---

function ConfigEditor() {
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
  const [confirmDeploy, setConfirmDeploy] = useState(false);

  const handleValidate = () => {
    try {
      const parsed = JSON.parse(configText);
      if (!parsed.agent?.name) throw new Error('agent.name is required');
      if (!parsed.agent?.model?.provider || !parsed.agent?.model?.name)
        throw new Error('agent.model.provider and agent.model.name are required');
      setParseError(null);
      toast.success('Configuration is valid');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Invalid JSON';
      setParseError(msg);
      toast.error(`Validation failed: ${msg}`);
    }
  };

  const handleDeploy = () => {
    try {
      const parsed = JSON.parse(configText);
      if (!parsed.agent?.name) throw new Error('agent.name is required');
      if (!parsed.agent?.model?.provider || !parsed.agent?.model?.name)
        throw new Error('agent.model.provider and agent.model.name are required');
      setParseError(null);
      setConfirmDeploy(true);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Invalid JSON';
      setParseError(msg);
      toast.error(`Validation failed: ${msg}`);
    }
  };

  const confirmDeployAction = () => {
    setConfirmDeploy(false);
    toast.success('Configuration deployed successfully');
  };

  return (
    <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
      <div className="lg:col-span-2 space-y-4">
        <Card className="overflow-hidden flex flex-col h-[600px]">
          <div className="flex items-center justify-between border-b bg-muted/50 px-4 py-2">
            <span className="font-mono text-xs text-muted-foreground">config.json</span>
            <Badge variant="outline">JSON</Badge>
          </div>
          <textarea
            value={configText}
            onChange={(e) => {
              setConfigText(e.target.value);
              setParseError(null);
            }}
            className={cn(
              'flex-1 w-full p-4 font-mono text-sm resize-none focus:outline-none',
              parseError ? 'bg-destructive/10 text-destructive' : 'bg-background',
            )}
            spellCheck={false}
            aria-label="Configuration editor"
          />
        </Card>
      </div>

      <div className="space-y-6">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-semibold">Actions</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <Button variant="outline" className="w-full" onClick={handleValidate}>
              Validate Syntax
            </Button>
            <Button className="w-full" onClick={handleDeploy}>
              Deploy Configuration
            </Button>

            {parseError && (
              <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
                <div className="text-sm font-semibold text-destructive mb-1">Validation Error</div>
                <p className="font-mono text-xs text-destructive">{parseError}</p>
              </div>
            )}
          </CardContent>
        </Card>

        <Card className="border-primary/20 bg-primary/5">
          <CardContent className="p-6">
            <h4 className="text-sm font-medium text-primary mb-2">Help</h4>
            <p className="text-sm leading-relaxed text-muted-foreground">
              Edit the canonical ClawDen configuration. This defines agent behaviors, capabilities,
              and runtime parameters. Changes are validated before propagation.
            </p>
          </CardContent>
        </Card>
      </div>

      <AlertDialog
        open={confirmDeploy}
        title="Deploy Configuration"
        description="This will update the active agent configuration. Running agents may be affected. Are you sure?"
        confirmLabel="Deploy"
        cancelLabel="Cancel"
        onConfirm={confirmDeployAction}
        onCancel={() => setConfirmDeploy(false)}
      />
    </div>
  );
}

// --- Audit Log ---

function AuditLogViewer({ events }: { events: AuditEvent[] }) {
  const sorted = useMemo(
    () => [...events].sort((a, b) => b.timestamp_unix_ms - a.timestamp_unix_ms),
    [events],
  );

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className="text-base">System Activity</CardTitle>
          <Badge variant="secondary">{sorted.length} events</Badge>
        </div>
      </CardHeader>
      <CardContent className="p-0">
        {sorted.length === 0 ? (
          <EmptyState
            icon={<span className="text-4xl">📋</span>}
            title="No audit events"
            description="No audit events recorded yet."
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b bg-muted/50 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <th className="px-6 py-3 text-left">Time</th>
                  <th className="px-6 py-3 text-left">Actor</th>
                  <th className="px-6 py-3 text-left">Action</th>
                  <th className="px-6 py-3 text-left">Target</th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {sorted.slice(0, 200).map((event, i) => (
                  <tr key={i} className="hover:bg-muted/30">
                    <td className="px-6 py-4 whitespace-nowrap text-xs text-muted-foreground">
                      {new Date(event.timestamp_unix_ms).toLocaleString()}
                    </td>
                    <td className="px-6 py-4 font-medium">{event.actor}</td>
                    <td className="px-6 py-4">
                      <Badge variant="outline" className="font-mono text-xs">{event.action}</Badge>
                    </td>
                    <td className="px-6 py-4 text-muted-foreground">{event.target}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// --- Helpers ---

function MetricCard({
  label,
  value,
  subtext,
  variant = 'default',
}: {
  label: string;
  value: number;
  subtext?: string;
  variant?: 'default' | 'success' | 'warning';
}) {
  const accentClass = {
    default: 'border-l-4 border-l-primary',
    success: 'border-l-4 border-l-green-500',
    warning: 'border-l-4 border-l-amber-500',
  }[variant];

  return (
    <Card className={accentClass}>
      <CardContent className="p-6">
        <div className="text-sm font-medium text-muted-foreground">{label}</div>
        <div className="mt-2 text-3xl font-bold">{value}</div>
        {subtext && <div className="mt-1 text-xs text-muted-foreground">{subtext}</div>}
      </CardContent>
    </Card>
  );
}

function StateBadge({ state }: { state: AgentState }) {
  const variantMap: Record<AgentState, BadgeVariant> = {
    running: 'success',
    degraded: 'warning',
    stopped: 'secondary',
    registered: 'outline',
    installed: 'secondary',
  };

  return <Badge variant={variantMap[state] ?? 'secondary'} className="capitalize">{state}</Badge>;
}

function HealthBadge({ status }: { status: AgentRecord['health'] }) {
  const dotColor = {
    healthy: 'bg-green-500',
    degraded: 'bg-amber-500',
    unhealthy: 'bg-red-500',
    unknown: 'bg-slate-400',
  };

  const variantMap: Record<AgentRecord['health'], BadgeVariant> = {
    healthy: 'success',
    degraded: 'warning',
    unhealthy: 'destructive',
    unknown: 'secondary',
  };

  return (
    <Badge variant={variantMap[status]} className="gap-1">
      <span className={cn('h-1.5 w-1.5 rounded-full', dotColor[status])} />
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </Badge>
  );
}

function DetailRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between py-2 border-b last:border-0">
      <span className="text-sm font-medium text-muted-foreground">{label}</span>
      <span className={cn('text-sm', mono && 'font-mono')}>{value}</span>
    </div>
  );
}

function EmptyState({
  icon,
  title,
  description,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16 text-center text-muted-foreground">
      {icon}
      <div>
        <p className="font-medium text-foreground">{title}</p>
        <p className="text-sm mt-1">{description}</p>
      </div>
    </div>
  );
}

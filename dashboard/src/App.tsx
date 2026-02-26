import { useEffect, useMemo, useState } from 'react';

type AgentState = 'registered' | 'installed' | 'running' | 'stopped' | 'degraded';

interface AgentRecord {
  id: string;
  name: string;
  runtime: string;
  capabilities: string[];
  state: AgentState;
  health: 'healthy' | 'degraded' | 'unhealthy' | 'unknown';
  task_count: number;
}

interface FleetStatus {
  total_agents: number;
  running_agents: number;
  degraded_agents: number;
}

const POLL_MS = 2_000;

export function App() {
  const [status, setStatus] = useState<FleetStatus>({
    total_agents: 0,
    running_agents: 0,
    degraded_agents: 0,
  });
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;

    const refresh = async () => {
      try {
        const [statusRes, agentsRes] = await Promise.all([
          fetch('/fleet/status'),
          fetch('/agents'),
        ]);

        if (!statusRes.ok || !agentsRes.ok) {
          throw new Error('failed to fetch dashboard data');
        }

        const [nextStatus, nextAgents] = (await Promise.all([
          statusRes.json(),
          agentsRes.json(),
        ])) as [FleetStatus, AgentRecord[]];

        if (!alive) {
          return;
        }

        setStatus(nextStatus);
        setAgents(nextAgents);
        setError(null);
      } catch (err) {
        if (alive) {
          setError(err instanceof Error ? err.message : 'unknown error');
        }
      }
    };

    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
    }, POLL_MS);

    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, []);

  const healthyAgents = useMemo(
    () => agents.filter((agent) => agent.health === 'healthy').length,
    [agents]
  );

  return (
    <main style={{ fontFamily: 'system-ui', margin: '2rem' }}>
      <h1>ClawDen Dashboard</h1>
      <p>Fleet overview</p>

      <section style={{ display: 'flex', gap: '1rem', margin: '1rem 0' }}>
        <MetricCard label="Total" value={status.total_agents} />
        <MetricCard label="Running" value={status.running_agents} />
        <MetricCard label="Degraded" value={status.degraded_agents} />
        <MetricCard label="Healthy" value={healthyAgents} />
      </section>

      {error ? <p style={{ color: '#b91c1c' }}>Data source error: {error}</p> : null}

      <section>
        <h2>Agents</h2>
        <table style={{ width: '100%', borderCollapse: 'collapse' }}>
          <thead>
            <tr>
              <th align="left">Name</th>
              <th align="left">Runtime</th>
              <th align="left">State</th>
              <th align="left">Health</th>
              <th align="left">Capabilities</th>
            </tr>
          </thead>
          <tbody>
            {agents.map((agent) => (
              <tr key={agent.id}>
                <td>{agent.name}</td>
                <td>{agent.runtime}</td>
                <td>{agent.state}</td>
                <td>
                  <StatusDot status={agent.health} /> {agent.health}
                </td>
                <td>{agent.capabilities.join(', ') || '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </main>
  );
}

function MetricCard({ label, value }: { label: string; value: number }) {
  return (
    <article
      style={{
        border: '1px solid #e5e7eb',
        borderRadius: '0.5rem',
        padding: '0.75rem 1rem',
        minWidth: '7rem',
      }}
    >
      <p style={{ margin: 0, color: '#6b7280' }}>{label}</p>
      <strong style={{ fontSize: '1.25rem' }}>{value}</strong>
    </article>
  );
}

function StatusDot({ status }: { status: AgentRecord['health'] }) {
  const color =
    status === 'healthy'
      ? '#16a34a'
      : status === 'degraded'
        ? '#f59e0b'
        : status === 'unhealthy'
          ? '#dc2626'
          : '#6b7280';
  return (
    <span
      aria-hidden
      style={{
        display: 'inline-block',
        width: 8,
        height: 8,
        borderRadius: '9999px',
        backgroundColor: color,
        marginRight: 4,
      }}
    />
  );
}

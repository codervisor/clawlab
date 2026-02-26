import { BarChart2, ClipboardList, Server, Settings } from 'lucide-react';
import { cn } from '../../lib/utils';
import { Tooltip } from '../ui/tooltip';

type View = 'fleet' | 'agent-detail' | 'tasks' | 'config' | 'audit';

interface SidebarProps {
  view: View;
  onNavigate: (view: View) => void;
  collapsed: boolean;
}

const NAV_ITEMS = [
  { view: 'fleet' as const, label: 'Fleet Overview', icon: Server },
  { view: 'tasks' as const, label: 'Task Monitor', icon: BarChart2 },
  { view: 'config' as const, label: 'Config Editor', icon: Settings },
  { view: 'audit' as const, label: 'Audit Log', icon: ClipboardList },
];

export function Sidebar({ view, onNavigate, collapsed }: SidebarProps) {
  const isActive = (v: View) => v === view || (v === 'fleet' && view === 'agent-detail');

  return (
    <aside
      className={cn(
        'flex flex-col border-r bg-[hsl(var(--sidebar-background))] text-[hsl(var(--sidebar-foreground))] transition-all duration-200',
        collapsed ? 'w-14' : 'w-60',
      )}
      aria-label="Main navigation"
    >
      {/* Logo */}
      <div className={cn('flex h-14 items-center border-b border-[hsl(var(--sidebar-border))]', collapsed ? 'justify-center px-2' : 'gap-2 px-4')}>
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary font-black text-primary-foreground">
          C
        </div>
        {!collapsed && (
          <span className="text-sm font-bold tracking-tight">ClawDen</span>
        )}
      </div>

      {/* Nav */}
      <nav className="flex-1 space-y-1 p-2" aria-label="Navigation">
        {NAV_ITEMS.map(({ view: v, label, icon: Icon }) => {
          const active = isActive(v);
          const btn = (
            <button
              key={v}
              onClick={() => onNavigate(v)}
              aria-label={label}
              aria-current={active ? 'page' : undefined}
              className={cn(
                'flex w-full items-center rounded-md px-2 py-2.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                active
                  ? 'bg-primary text-primary-foreground'
                  : 'text-[hsl(var(--sidebar-foreground))]/70 hover:bg-[hsl(var(--sidebar-accent))] hover:text-[hsl(var(--sidebar-accent-foreground))]',
                collapsed ? 'justify-center' : 'gap-3',
              )}
            >
              <Icon className="h-4 w-4 shrink-0" aria-hidden="true" />
              {!collapsed && <span>{label}</span>}
            </button>
          );

          return collapsed ? (
            <Tooltip key={v} content={label}>{btn}</Tooltip>
          ) : (
            <div key={v}>{btn}</div>
          );
        })}
      </nav>

      {/* Footer */}
      {!collapsed && (
        <div className="border-t border-[hsl(var(--sidebar-border))] p-4">
          <div className="text-xs text-[hsl(var(--sidebar-foreground))]/50">v0.1.0 • @codervisor</div>
        </div>
      )}
    </aside>
  );
}

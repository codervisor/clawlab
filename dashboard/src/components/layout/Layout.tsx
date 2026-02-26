import { useState } from 'react';
import { Toaster } from 'sonner';
import { Header } from './Header';
import { Sidebar } from './Sidebar';
import { Sheet } from '../ui/sheet';
import { ScrollArea } from '../ui/scroll-area';
import { useSidebar } from '../../hooks/useSidebar';
import { useTheme } from '../../hooks/useTheme';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';

type View = 'fleet' | 'agent-detail' | 'tasks' | 'config' | 'audit';

const VIEW_TITLES: Record<View, string> = {
  fleet: 'Fleet Overview',
  'agent-detail': 'Agent Details',
  tasks: 'Task Monitor',
  config: 'Configuration',
  audit: 'Audit Log',
};

interface LayoutProps {
  view: View;
  onNavigate: (view: View) => void;
  wsConnected: boolean;
  children: React.ReactNode;
}

export function Layout({ view, onNavigate, wsConnected, children }: LayoutProps) {
  const { collapsed, toggle } = useSidebar();
  const [mobileOpen, setMobileOpen] = useState(false);
  useTheme();

  useKeyboardShortcuts([
    { key: 'b', meta: true, handler: toggle },
  ]);

  return (
    <div className="flex h-screen overflow-hidden bg-background">
      {/* Desktop sidebar */}
      <div className="hidden lg:flex">
        <Sidebar
          view={view}
          onNavigate={onNavigate}
          collapsed={collapsed}
        />
      </div>

      {/* Mobile sidebar (Sheet) */}
      <Sheet open={mobileOpen} onClose={() => setMobileOpen(false)}>
        <Sidebar
          view={view}
          onNavigate={(v) => { onNavigate(v); setMobileOpen(false); }}
          collapsed={false}
        />
      </Sheet>

      {/* Main area */}
      <div className="flex flex-1 flex-col overflow-hidden">
        <Header
          title={VIEW_TITLES[view]}
          onMenuClick={() => {
            if (window.innerWidth >= 1024) {
              toggle();
            } else {
              setMobileOpen(true);
            }
          }}
          wsConnected={wsConnected}
        />

        <ScrollArea className="flex-1">
          <main className="p-6 max-w-7xl mx-auto">
            {children}
          </main>
        </ScrollArea>
      </div>

      <Toaster position="bottom-right" richColors closeButton />
    </div>
  );
}

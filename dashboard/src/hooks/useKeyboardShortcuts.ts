import { useEffect } from 'react';

interface Shortcut {
  key: string;
  meta?: boolean;
  ctrl?: boolean;
  shift?: boolean;
  handler: () => void;
}

export function useKeyboardShortcuts(shortcuts: Shortcut[]) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      for (const shortcut of shortcuts) {
        const needsMeta = shortcut.meta === true;
        const needsShift = shortcut.shift === true;
        const hasModifier = e.metaKey || e.ctrlKey;

        const metaMatch = needsMeta ? hasModifier : true;
        const shiftMatch = needsShift ? e.shiftKey : true;
        const keyMatch = e.key.toLowerCase() === shortcut.key.toLowerCase();

        if (metaMatch && shiftMatch && keyMatch) {
          e.preventDefault();
          shortcut.handler();
          break;
        }
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [shortcuts]);
}

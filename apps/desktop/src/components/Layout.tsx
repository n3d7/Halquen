import type { ReactNode } from "react";
import {
  Activity,
  Bot,
  Brain,
  HeartPulse,
  MessageSquare,
  Settings,
} from "lucide-react";
import { StatusBadge } from "./Common";

export type Page = "chat" | "activity" | "memory" | "ai" | "diagnostics" | "settings";

const navigation: Array<{ page: Page; label: string; icon: typeof MessageSquare }> = [
  { page: "chat", label: "Chat", icon: MessageSquare },
  { page: "activity", label: "Activity", icon: Activity },
  { page: "memory", label: "Memory", icon: Brain },
  { page: "ai", label: "AI", icon: Bot },
  { page: "diagnostics", label: "Diagnostics", icon: HeartPulse },
  { page: "settings", label: "Settings", icon: Settings },
];

export function Layout({
  page,
  onNavigate,
  daemonOnline,
  children,
}: {
  page: Page;
  onNavigate: (page: Page) => void;
  daemonOnline: boolean;
  children: ReactNode;
}) {
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">H</div>
          <div>
            <strong>Halquen</strong>
            <span>Local-first assistant</span>
          </div>
        </div>
        <nav aria-label="Main navigation">
          {navigation.map(({ page: target, label, icon: Icon }) => (
            <button
              key={target}
              className={page === target ? "nav-item active" : "nav-item"}
              onClick={() => onNavigate(target)}
              aria-current={page === target ? "page" : undefined}
            >
              <Icon size={18} strokeWidth={1.8} />
              <span>{label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-status">
          <span>Daemon</span>
          <StatusBadge tone={daemonOnline ? "good" : "bad"}>
            {daemonOnline ? "Connected" : "Unavailable"}
          </StatusBadge>
        </div>
      </aside>
      <main className="main-content">{children}</main>
    </div>
  );
}

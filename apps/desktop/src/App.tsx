import { useCallback, useEffect, useState } from "react";
import { WifiOff } from "lucide-react";
import { Button } from "./components/Common";
import { Layout, type Page } from "./components/Layout";
import { daemon } from "./lib/daemon";
import type { ApplicationSettings } from "./lib/types";
import { ActivityScreen } from "./screens/ActivityScreen";
import { AiScreen } from "./screens/AiScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { DiagnosticsScreen } from "./screens/DiagnosticsScreen";
import { MemoryScreen } from "./screens/MemoryScreen";
import { SettingsScreen } from "./screens/SettingsScreen";

function applyAppearance(appearance: ApplicationSettings["appearance"]) {
  document.documentElement.dataset.theme = appearance;
}

export default function App() {
  const [page, setPage] = useState<Page>("chat");
  const [daemonOnline, setDaemonOnline] = useState(false);
  const [checking, setChecking] = useState(true);

  const checkHealth = useCallback(async () => {
    setChecking(true);
    try {
      const health = await daemon.health();
      setDaemonOnline(health.status === "ok");
      try {
        const settings = await daemon.settings();
        applyAppearance(settings.appearance);
      } catch {
        // Health is authoritative for connection state; theme retrieval is optional here.
      }
    } catch {
      setDaemonOnline(false);
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => { void checkHealth(); }, [checkHealth]);

  useEffect(() => {
    function shortcut(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key === ",") {
        event.preventDefault();
        setPage("settings");
      }
    }
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, []);

  let content;
  switch (page) {
    case "chat":
      content = <ChatScreen daemonOnline={daemonOnline} onOpenAi={() => setPage("ai")} />;
      break;
    case "activity":
      content = <ActivityScreen />;
      break;
    case "memory":
      content = <MemoryScreen />;
      break;
    case "ai":
      content = <AiScreen />;
      break;
    case "diagnostics":
      content = <DiagnosticsScreen />;
      break;
    case "settings":
      content = <SettingsScreen onAppearanceChange={applyAppearance} />;
      break;
  }

  return (
    <Layout page={page} onNavigate={setPage} daemonOnline={daemonOnline}>
      {!daemonOnline && !checking && page !== "chat" ? (
        <div className="offline-banner" role="status">
          <WifiOff size={17} />
          <span>The daemon is unavailable. Local data and settings will reconnect when it starts.</span>
          <Button variant="ghost" onClick={() => void checkHealth()}>Retry</Button>
        </div>
      ) : null}
      {content}
    </Layout>
  );
}

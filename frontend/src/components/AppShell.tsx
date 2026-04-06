import type { PropsWithChildren } from "react";
import { Sidebar } from "./Sidebar";
import type { ViewName } from "../types";

interface AppShellProps extends PropsWithChildren {
  currentView: ViewName;
  connectedCount: number;
  bridgeReady: boolean;
  onChangeView: (view: ViewName) => void;
}

export function AppShell({ children, currentView, connectedCount, bridgeReady, onChangeView }: AppShellProps) {
  return (
    <>
      <Sidebar
        currentView={currentView}
        connectedCount={connectedCount}
        bridgeReady={bridgeReady}
        onChangeView={onChangeView}
      />
      <div id="main">{children}</div>
    </>
  );
}

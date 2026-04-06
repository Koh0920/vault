import { useMemo } from "react";
import { useAppDispatch, useAppState, useToastActions } from "../context/AppContext";
import type { Bridge } from "../lib/bridge";
import type { ProviderId } from "../types";

export function useProviders(bridge: Bridge) {
  const state = useAppState();
  const dispatch = useAppDispatch();
  const toast = useToastActions();

  return useMemo(
    () => ({
      providers: state.providers,
      async loadStatuses() {
        const providerStatuses = await bridge.getProviderStatuses().catch(() => null);
        if (!providerStatuses) return;
        dispatch({ type: "set-providers-from-status", providers: providerStatuses.providers });
      },
      async connect(provider: ProviderId) {
        dispatch({
          type: "set-provider",
          provider,
          state: {
            status: "pending",
            meta: provider === "drive" ? "ブラウザで認証中…" : "設定を確認中…",
          },
        });
        try {
          const result = await bridge.connectProvider(provider);
          dispatch({
            type: "set-provider",
            provider,
            state: {
              status: result.ok ? "connected" : "failed",
              meta: result.nextAction,
            },
          });
          toast.push(
            result.ok
              ? `${provider === "drive" ? "Google Drive" : "Cloudflare R2"} の接続が完了しました`
              : result.nextAction,
            result.ok ? "ok" : "err",
          );
        } catch (error) {
          dispatch({
            type: "set-provider",
            provider,
            state: {
              status: "failed",
              meta: String(error),
            },
          });
          toast.push(String(error), "err");
        }
      },
    }),
    [bridge, dispatch, state.providers, toast],
  );
}

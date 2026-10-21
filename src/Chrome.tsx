import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MouseEvent } from "react";
import { initialLocale } from "./i18n";
import type { DesktopLocale } from "./i18n";
import logo from "./assets/huimeng.png";
import "./Chrome.css";

type ChromeState = {
  locale: DesktopLocale;
  maximized: boolean;
  focused: boolean;
};

type WindowAction = "menu" | "minimize" | "toggle-maximize" | "close" | "start-dragging";

const labels = {
  "zh-CN": {
    brand: "慧梦",
    workspace: "创作工作台",
    menu: "应用菜单",
    minimize: "最小化",
    maximize: "最大化",
    restore: "向下还原",
    close: "关闭窗口",
    error: "操作未完成，请重试",
    retry: "重试",
  },
  en: {
    brand: "Huimeng",
    workspace: "Creative workspace",
    menu: "App menu",
    minimize: "Minimize",
    maximize: "Maximize",
    restore: "Restore down",
    close: "Close window",
    error: "Unable to complete this action",
    retry: "Retry",
  },
} as const;

function Chrome() {
  const [state, setState] = useState<ChromeState>({
    locale: initialLocale(),
    maximized: false,
    focused: true,
  });
  const [failure, setFailure] = useState<WindowAction | "state" | null>(null);
  const text = labels[state.locale];

  const refresh = useCallback(async () => {
    const next = await invoke<ChromeState>("get_chrome_state");
    setState(next);
  }, []);

  useEffect(() => {
    document.documentElement.lang = state.locale;
    document.title = text.brand;
  }, [state.locale, text.brand]);

  useEffect(() => {
    let active = true;
    const subscriptions: UnlistenFn[] = [];
    const currentWindow = getCurrentWindow();
    const update = () => {
      invoke<ChromeState>("get_chrome_state").then((next) => {
        if (active) setState(next);
      }).catch(() => {
        if (active) setFailure("state");
      });
    };
    update();
    const listeners = [
      currentWindow.onResized(update),
      currentWindow.onFocusChanged(({ payload }) => {
        if (active) setState((current) => ({ ...current, focused: payload }));
      }),
    ];
    for (const listener of listeners) {
      listener.then((unlisten) => {
        if (active) subscriptions.push(unlisten);
        else unlisten();
      }).catch(() => {
        if (active) setFailure("state");
      });
    }
    return () => {
      active = false;
      subscriptions.forEach((unlisten) => unlisten());
    };
  }, []);

  async function perform(action: WindowAction) {
    setFailure(null);
    try {
      await invoke("desktop_window_action", { action });
      if (action === "toggle-maximize") await refresh();
    } catch {
      setFailure(action);
    }
  }

  function drag(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
    event.preventDefault();
    void perform(event.detail === 2 ? "toggle-maximize" : "start-dragging");
  }

  return (
    <header
      className={`desktop-chrome${state.focused ? "" : " is-unfocused"}`}
      onMouseDown={drag}
      onContextMenu={(event) => event.preventDefault()}
      aria-label={text.brand}
    >
      <div className="chrome-brand" aria-hidden="true">
        <img src={logo} alt="" draggable={false} />
        <span>{text.brand}</span>
      </div>
      <button
        className="chrome-menu"
        type="button"
        aria-label={text.menu}
        aria-haspopup="menu"
        title={text.menu}
        onClick={() => void perform("menu")}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 4h10M3 8h10M3 12h10" /></svg>
      </button>
      <span className="chrome-divider" aria-hidden="true" />
      <span className="chrome-workspace" aria-hidden="true">{text.workspace}</span>
      <div className="chrome-drag-space" />
      {failure && (
        <div className="chrome-feedback" role="alert">
          <span>{text.error}</span>
          <button type="button" onClick={() => {
            if (failure === "state") {
              refresh().then(() => setFailure(null)).catch(() => setFailure("state"));
            } else {
              void perform(failure);
            }
          }}>{text.retry}</button>
        </div>
      )}
      <div className="chrome-window-controls">
        <button type="button" aria-label={text.minimize} title={text.minimize} onClick={() => void perform("minimize")}>
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M1 6.5h10" /></svg>
        </button>
        <button
          type="button"
          aria-label={state.maximized ? text.restore : text.maximize}
          title={state.maximized ? text.restore : text.maximize}
          onClick={() => void perform("toggle-maximize")}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            {state.maximized ? <path d="M3.5 3.5v-2h7v7h-2m-7-5h7v7h-7z" /> : <path d="M1.5 1.5h9v9h-9z" />}
          </svg>
        </button>
        <button className="chrome-close" type="button" aria-label={text.close} title={text.close} onClick={() => void perform("close")}>
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m1.5 1.5 9 9m0-9-9 9" /></svg>
        </button>
      </div>
    </header>
  );
}

export default Chrome;

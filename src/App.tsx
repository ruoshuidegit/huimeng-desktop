import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { initialLocale, messages } from "./i18n";
import type { DesktopLocale } from "./i18n";
import "./App.css";

type ProxySettings = {
  mode: "system" | "manual";
  protocol: "http" | "socks5";
  host: string;
  port: number;
};

const DEFAULT_SETTINGS: ProxySettings = {
  mode: "system",
  protocol: "http",
  host: "127.0.0.1",
  port: 7897,
};

function publicError(error: unknown, fallback: string): string {
  return typeof error === "string" ? error : fallback;
}

function App() {
  const [locale, setLocale] = useState<DesktopLocale>(initialLocale);
  const text = messages[locale];
  const [settings, setSettings] = useState(DEFAULT_SETTINGS);
  const [port, setPort] = useState(String(DEFAULT_SETTINGS.port));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");
  const [localeFailed, setLocaleFailed] = useState(false);
  const [loadAttempt, setLoadAttempt] = useState(0);

  useEffect(() => {
    document.documentElement.lang = locale;
    document.title = messages[locale].title;
  }, [locale]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    setSuccess("");
    Promise.allSettled([
      invoke<string>("get_desktop_locale"),
      invoke<ProxySettings>("get_proxy_settings"),
    ]).then(([localeResult, settingsResult]) => {
      if (!active) return;
      const language = localeResult.status === "fulfilled"
        && (localeResult.value === "zh-CN" || localeResult.value === "en")
        ? localeResult.value
        : initialLocale();
      const languageFailed = localeResult.status === "rejected"
        || (localeResult.value !== "zh-CN" && localeResult.value !== "en");
      setLocale(language);
      setLocaleFailed(languageFailed);
      if (settingsResult.status === "fulfilled") {
        setSettings(settingsResult.value);
        setPort(String(settingsResult.value.port));
      }
      if (languageFailed) {
        setError(messages[language].localeLoadError);
      } else if (settingsResult.status === "rejected") {
        setError(messages[language].settingsLoadError);
      }
      setLoading(false);
    });
    return () => {
      active = false;
    };
  }, [loadAttempt]);

  function updateSettings(patch: Partial<ProxySettings>) {
    setSettings((current) => ({ ...current, ...patch }));
    setError("");
    setSuccess("");
  }

  async function save() {
    if (loading || saving || localeFailed) return;
    setError("");
    setSuccess("");

    const host = settings.host.trim();
    const parsedPort = Number(port);
    if (settings.mode === "manual") {
      if (!host || /[\s/@?#\\]/.test(host) || host.includes("://")) {
        setError(text.invalidHost);
        return;
      }
      if (!/^\d+$/.test(port) || !Number.isInteger(parsedPort) || parsedPort < 1 || parsedPort > 65535) {
        setError(text.invalidPort);
        return;
      }
    }

    setSaving(true);
    try {
      const saved = await invoke<ProxySettings>("save_proxy_settings", {
        settings: {
          ...settings,
          host,
          port: settings.mode === "manual" ? parsedPort : settings.port,
        },
      });
      setSettings(saved);
      setPort(String(saved.port));
      setSuccess(text.saved);
    } catch (err) {
      setError(publicError(err, text.saveError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <main className="settings-page">
      <header className="settings-header">
        <span className="eyebrow">{text.eyebrow}</span>
        <h1>{text.heading}</h1>
        <p>{text.description}</p>
      </header>

      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        aria-busy={loading || saving}
      >
        <fieldset disabled={loading || saving || localeFailed}>
          <legend>{text.connection}</legend>
          <div className="mode-options">
            <label className={`mode-option ${settings.mode === "system" ? "selected" : ""}`}>
              <input
                type="radio"
                name="proxy-mode"
                value="system"
                checked={settings.mode === "system"}
                onChange={() => updateSettings({ mode: "system" })}
              />
              <span>
                <strong>{text.system}</strong>
                <span className="option-description">{text.systemDescription}</span>
              </span>
            </label>
            <label className={`mode-option ${settings.mode === "manual" ? "selected" : ""}`}>
              <input
                type="radio"
                name="proxy-mode"
                value="manual"
                checked={settings.mode === "manual"}
                onChange={() => updateSettings({ mode: "manual" })}
              />
              <span>
                <strong>{text.manual}</strong>
                <span className="option-description">{text.manualDescription}</span>
              </span>
            </label>
          </div>

          {settings.mode === "manual" && (
            <div className="manual-settings">
              <label className="field" htmlFor="proxy-protocol">
                <span>{text.protocol}</span>
                <select
                  id="proxy-protocol"
                  value={settings.protocol}
                  onChange={(event) => updateSettings({ protocol: event.target.value as ProxySettings["protocol"] })}
                  aria-describedby="protocol-help"
                >
                  <option value="http">HTTP</option>
                  <option value="socks5">SOCKS5</option>
                </select>
              </label>
              <p id="protocol-help" className="field-help">{text.protocolHelp}</p>
              <div className="address-row">
                <label className="field host-field" htmlFor="proxy-host">
                  <span>{text.host}</span>
                  <input
                    id="proxy-host"
                    type="text"
                    value={settings.host}
                    placeholder="127.0.0.1"
                    spellCheck={false}
                    autoCapitalize="none"
                    autoComplete="off"
                    onChange={(event) => updateSettings({ host: event.target.value })}
                    aria-describedby="host-help"
                  />
                </label>
                <label className="field port-field" htmlFor="proxy-port">
                  <span>{text.port}</span>
                  <input
                    id="proxy-port"
                    type="text"
                    inputMode="numeric"
                    value={port}
                    placeholder="7897"
                    autoComplete="off"
                    onChange={(event) => {
                      setPort(event.target.value);
                      setError("");
                      setSuccess("");
                    }}
                    aria-describedby="host-help"
                  />
                </label>
              </div>
              <p id="host-help" className="field-help">{text.hostHelp}</p>
            </div>
          )}
        </fieldset>

        <p className="restart-note">{text.restartNote}</p>
        <div className="feedback" aria-live="polite" aria-atomic="true">
          {loading && <p className="loading-message" role="status">{text.loading}</p>}
          {error && <p className="error-message" role="alert">{error}</p>}
          {success && <p className="success-message" role="status">{success}</p>}
        </div>
        <footer className="settings-actions">
          {localeFailed && !loading && (
            <button type="button" onClick={() => setLoadAttempt((attempt) => attempt + 1)}>
              {text.retry}
            </button>
          )}
          <button className="primary-button" type="submit" disabled={loading || saving || localeFailed}>
            {saving ? text.saving : text.save}
          </button>
        </footer>
      </form>
    </main>
  );
}

export default App;

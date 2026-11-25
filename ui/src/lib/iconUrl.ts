import type { Os } from "../bindings";

/**
 * URL for a mod icon served by the backend's `svsm` protocol. WebView2 on Windows can't load
 * custom schemes directly, so Tauri maps them to `http://<scheme>.localhost`.
 */
export function iconUrl(os: Os, key: string): string {
  return os === "windows" ? `http://svsm.localhost/icon/${key}` : `svsm://localhost/icon/${key}`;
}

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { BrowseEntry, ModDbScreenshot, UpdateProgress } from "../../bindings";
import { useInstallMod, useModDbDetails, useOpenUrl, useUpdateProgress } from "../../api";
import { formatCount, formatDate } from "../../lib/format";
import { linkClickHandler, sanitizeHtml } from "../../lib/html";
import { Icon } from "../../components/Icon";
import { progressLabel } from "../mods/InstalledMods";
import { summarizeVersions } from "../mods/UpdateDialog";
import { Lightbox } from "./ScreenshotGallery";
import { useErrorToast } from "../../components/Toast";

/**
 * A mod from the Mod DB in a window over the browser, as 2.3 did it: the first screenshot as
 * the big picture (like the mod's page on the Mod DB), the facts and install button, then the
 * author's description rendered as (sanitized) HTML.
 */
export function ModDetailsWindow({ entry, thumbUrl, offline, onClose, onFavourite, favouriteBusy }: {
  entry: BrowseEntry;
  thumbUrl: string | null;
  offline: boolean;
  onClose: () => void;
  onFavourite: () => void;
  favouriteBusy: boolean;
}) {
  const titleId = useId();
  const details = useModDbDetails(entry.modId);
  const install = useInstallMod();
  const openUrl = useOpenUrl();
  useErrorToast(install.error ?? openUrl.error);
  const [picked, setPicked] = useState("");
  const [progress, setProgress] = useState<string | null>(null);
  const [shot, setShot] = useState(0);
  const [zoomed, setZoomed] = useState<number | null>(null);
  const windowRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setPicked("");
    setShot(0);
    setZoomed(null);
    install.reset();
    windowRef.current?.focus();
    windowRef.current?.querySelector(".mod-window-scroll")?.scrollTo(0, 0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entry.modId]);

  const onProgress = useCallback(
    (p: UpdateProgress) => {
      if (p.key === `install:${entry.modId}`) setProgress(progressLabel(p));
    },
    [entry.modId],
  );
  useUpdateProgress(onProgress);

  const info = details.data?.info;
  const screenshots: ModDbScreenshot[] = offline ? [] : (info?.screenshots ?? []);
  const count = screenshots.length;
  const step = (n: number) => setShot((i) => (i + n + count) % count);

  // Escape closes (the lightbox handles its own keys while open); arrows flip screenshots.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (zoomed !== null) return;
      const typing = e.target instanceof Element && !!e.target.closest("input, select, textarea");
      if (e.key === "Escape") onClose();
      else if (!typing && count > 1 && e.key === "ArrowRight") step(1);
      else if (!typing && count > 1 && e.key === "ArrowLeft") step(-1);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const latestCompatible = info?.latestCompatibleIndex != null ? info.releases[info.latestCompatibleIndex] : null;
  const chosen = picked ? info?.releases.find((r) => r.version === picked) : latestCompatible;
  const html = useMemo(
    () => (details.data?.descriptionHtml ? sanitizeHtml(details.data.descriptionHtml) : null),
    [details.data?.descriptionHtml],
  );
  const pageUrl = info?.pageUrl
    ?? (entry.urlAlias ? `https://mods.vintagestory.at/${encodeURIComponent(entry.urlAlias)}`
      : entry.assetId != null ? `https://mods.vintagestory.at/show/mod/${entry.assetId}` : null);
  const links = [
    ["Homepage", info?.homepageUrl],
    ["Source code", info?.sourceCodeUrl],
    ["Issues", info?.issueTrackerUrl],
    ["Wiki", info?.wikiUrl],
  ].filter((l): l is [string, string] => !!l[1]);
  const heroUrl = count > 0 ? screenshots[Math.min(shot, count - 1)].url : thumbUrl;

  return createPortal(
    <div className="mod-window-backdrop" role="presentation" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={windowRef} className="mod-window" role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}>
        <div className="mod-window-scroll">
          <div className={`mod-window-hero${count > 0 ? " has-shots" : ""}`}>
            {heroUrl ? (
              <>
                <img className="backdrop" src={heroUrl} alt="" aria-hidden />
                <img
                  key={heroUrl}
                  className="main"
                  src={heroUrl}
                  alt={count > 0 ? `Screenshot ${shot + 1} of ${count}` : ""}
                  onClick={count > 0 ? () => setZoomed(shot) : undefined}
                  title={count > 0 ? "View full size" : undefined}
                />
              </>
            ) : (
              <span className="mod-window-initial" aria-hidden>{entry.name.slice(0, 1).toUpperCase()}</span>
            )}
            {count > 1 && (
              <>
                <button type="button" className="hero-nav prev" onClick={() => step(-1)} aria-label="Previous screenshot"><Icon name="chevronLeft" size={22} /></button>
                <button type="button" className="hero-nav next" onClick={() => step(1)} aria-label="Next screenshot"><Icon name="chevronRight" size={22} /></button>
                <span className="hero-counter">{shot + 1} / {count}</span>
              </>
            )}
            <button type="button" className="hero-close" onClick={onClose} aria-label="Close" title="Close (Esc)">
              <Icon name="close" />
            </button>
          </div>

          <div className="mod-window-head">
            <div className="mod-window-title">
              <h2 id={titleId}>{entry.name}</h2>
              <p className="muted">
                {entry.author ? <>by <strong>{entry.author}</strong></> : "Unknown author"}
                {entry.side && <> · {sideLabel(entry.side)}</>}
                {entry.lastReleasedMs != null && <> · updated {formatDate(entry.lastReleasedMs)}</>}
              </p>
              {entry.tags.length > 0 && (
                <p className="tag-list">
                  {entry.tags.map((t) => <span key={t} className="badge">{t}</span>)}
                </p>
              )}
            </div>
            <dl className="mod-window-stats">
              <div title={`${entry.downloads.toLocaleString()} downloads`}><dt><Icon name="download" /></dt><dd>{formatCount(entry.downloads)}</dd></div>
              <div title={`${entry.follows.toLocaleString()} follows`}><dt aria-label="Follows">♥</dt><dd>{formatCount(entry.follows)}</dd></div>
              <div title={`${entry.comments.toLocaleString()} comments`}><dt><Icon name="comment" /></dt><dd>{formatCount(entry.comments)}</dd></div>
            </dl>
          </div>

          <div className="mod-window-actions">
            {entry.installed ? (
              <span className="badge installed-badge">Installed</span>
            ) : (
              <>
                <button
                  type="button"
                  className="primary"
                  disabled={!chosen || install.isPending || install.isSuccess}
                  title={chosen && !chosen.compatible ? "Not tagged for your game version" : undefined}
                  onClick={() => chosen && install.mutate({ modId: entry.modId, version: chosen.version })}
                >
                  <Icon name="download" />{install.isSuccess ? "Installed" : install.isPending ? (progress ?? "Installing…") : `Install ${chosen?.version ?? ""}`}
                </button>
                {info && info.releases.length > 1 && (
                  <select aria-label="Version" value={picked} onChange={(e) => setPicked(e.target.value)}>
                    <option value="">{latestCompatible ? `Newest for your game (${latestCompatible.version})` : "Choose a version"}</option>
                    {info.releases.map((r) => (
                      <option key={r.version} value={r.version}>
                        {r.version}
                        {r.compatible ? "" : " (other game version)"}
                      </option>
                    ))}
                  </select>
                )}
              </>
            )}
            <button type="button" disabled={favouriteBusy} onClick={onFavourite}>
              <Icon name="star" filled={entry.favourite} />Favourite
            </button>
            {pageUrl && (
              <button type="button" onClick={() => openUrl.mutate(pageUrl)}><Icon name="external" />Open in web browser</button>
            )}
          </div>
          {entry.installed && <p className="muted small mod-window-note">Already installed.</p>}
          {chosen && !chosen.compatible && (
            <p className="notice warn-text small mod-window-note">
              {chosen.version} is tagged for {summarizeVersions(chosen.gameVersions)}, not your target game version.
            </p>
          )}
          {!latestCompatible && info && info.releases.length > 0 && !picked && !entry.installed && (
            <p className="notice warn-text small mod-window-note">No release for your game version.</p>
          )}
          {links.length > 0 && (
            <p className="link-row small mod-window-note">
              {links.map(([label, url]) => (
                <button key={label} type="button" className="link-button" onClick={() => openUrl.mutate(url)}>
                  <Icon name="external" size={13} />{label}
                </button>
              ))}
            </p>
          )}

          <div className="mod-window-body">
            {details.isPending && <p className="muted">Loading description…</p>}
            {details.isError && <p className="error-text small">{details.error.message}</p>}
            {details.data && !info && (
              <p className="notice warn-text small">
                {details.data.error ??
                  (offline ? "Not available in offline mode." : "This mod has no downloadable releases.")}
              </p>
            )}
            {html ? (
              <div
                className="mod-description mod-window-description"
                onClick={linkClickHandler((url) => openUrl.mutate(url))}
                // Sanitized with DOMPurify above.
                dangerouslySetInnerHTML={{ __html: html }}
              />
            ) : (
              details.data && <p className="muted">{entry.summary ?? "No description."}</p>
            )}
            {chosen?.changelog && (
              <details className="details-section mod-window-changelog">
                <summary>Changes in {chosen.version}</summary>
                <pre className="changelog">{chosen.changelog}</pre>
              </details>
            )}
          </div>
        </div>
      </div>
      {zoomed !== null && count > 0 && (
        <Lightbox screenshots={screenshots} index={zoomed} onIndex={(i) => { setZoomed(i); setShot(i); }}
          onClose={() => setZoomed(null)} modName={entry.name} />
      )}
    </div>,
    document.body,
  );
}

function sideLabel(side: string): string {
  switch (side.toLowerCase()) {
    case "both": return "client and server";
    case "client": return "client only";
    case "server": return "server only";
    default: return side;
  }
}

// Mod DB descriptions are user-written HTML. Sanitize before rendering and never let links
// navigate the app window: the caller opens them in the system browser instead.
import DOMPurify from "dompurify";

export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "iframe", "video", "audio"],
    FORBID_ATTR: ["style", "class", "id"],
  });
}

/** Click handler for a container of sanitized HTML: opens http(s) links via `open`. */
export function linkClickHandler(open: (url: string) => void) {
  return (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) open(href);
  };
}

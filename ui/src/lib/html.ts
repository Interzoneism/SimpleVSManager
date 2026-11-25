// Mod DB descriptions are user-written HTML. Sanitize before rendering and never let links
// navigate the app window: the caller opens them in the system browser instead.
import DOMPurify from "dompurify";

/**
 * Inline styles authors use for layout (icon sizes, centring, rounded images...). Anything
 * that could lift content out of the description (position, z-index) or load from a style
 * (url()) is dropped.
 */
const STYLE_PROPS = new Set([
  "width", "height", "max-width", "max-height", "min-width", "min-height",
  "margin", "margin-top", "margin-right", "margin-bottom", "margin-left",
  "padding", "padding-top", "padding-right", "padding-bottom", "padding-left",
  "text-align", "vertical-align", "float", "clear", "display", "object-fit",
  "font-size", "font-weight", "font-style", "text-decoration", "text-decoration-line", "line-height",
  "border", "border-width", "border-style", "border-color", "border-radius",
  "border-top", "border-right", "border-bottom", "border-left",
  "gap", "flex", "flex-direction", "flex-wrap", "align-items", "justify-content", "list-style-type",
  "white-space", "aspect-ratio",
]);
const DISPLAY_VALUES = new Set(["block", "inline", "inline-block", "flex", "inline-flex", "none", "list-item"]);

function cleanStyle(style: CSSStyleDeclaration): string {
  const kept: string[] = [];
  // Colours only survive as a pair on one element with a solid background; otherwise text
  // tuned for the Mod DB's light page would end up dark on dark (or light on light).
  const bg = style.getPropertyValue("background-color").trim();
  const ownColours = !!style.getPropertyValue("color").trim() && !!bg && !/^(initial|inherit|unset|transparent|rgba\(.*,\s*0\))$/i.test(bg);
  for (let i = 0; i < style.length; i++) {
    const prop = style.item(i);
    const value = style.getPropertyValue(prop).trim();
    if (!value || /url\s*\(|expression|javascript:/i.test(value)) continue;
    if (prop === "display" && !DISPLAY_VALUES.has(value)) continue;
    const colour = prop === "color" || prop === "background-color";
    if (STYLE_PROPS.has(prop) || (colour && ownColours)) kept.push(`${prop}: ${value}`);
  }
  return kept.join("; ");
}

const purify = DOMPurify();
purify.addHook("afterSanitizeAttributes", (node) => {
  if (!(node instanceof Element) || !node.hasAttribute("style")) return;
  const style = (node as HTMLElement).style;
  const cleaned = style ? cleanStyle(style) : "";
  if (cleaned) node.setAttribute("style", cleaned);
  else node.removeAttribute("style");
});

export function sanitizeHtml(html: string): string {
  return purify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "iframe", "video", "audio"],
    FORBID_ATTR: ["class", "id"],
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

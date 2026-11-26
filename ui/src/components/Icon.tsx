const paths = {
  library: "M4 5h6v6H4z M14 5h6v6h-6z M4 15h6v5H4z M14 15h6v5h-6z",
  lists: "M8 6h12 M8 12h12 M8 18h12 M4 6h.01 M4 12h.01 M4 18h.01",
  layers: "m12 3 9 5-9 5-9-5 9-5Z M3 12l9 5 9-5 M3 16l9 5 9-5",
  history: "M3 12a9 9 0 1 1 2.6 6.4 M3 5v7h7 M12 7v5l3 2",
  compass: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z M15.5 8.5l-2 5-5 2 2-5 5-2Z",
  community: "M15 20v-2a4 4 0 0 0-4-4H7a4 4 0 0 0-4 4v2 M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z M21 20v-2a4 4 0 0 0-3-3.9 M17 3.1a4 4 0 0 1 0 7.8",
  config: "M9 5H7a2 2 0 0 0-2 2v3l-2 2 2 2v3a2 2 0 0 0 2 2h2 M15 5h2a2 2 0 0 1 2 2v3l2 2-2 2v3a2 2 0 0 1-2 2h-2",
  settings: "M4 7h16 M4 17h16 M8 4v6 M16 14v6",
  sidebar: "M3 5h18v14H3z M9 5v14",
  help: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z M9.1 9a3 3 0 0 1 5.8 1c0 2-3 2.5-3 4 M12 17.5h.01",
  play: "M8 5l12 7-12 7z",
  search: "M16 10a6 6 0 1 1-12 0 6 6 0 0 1 12 0Z M14.5 14.5 20 20",
  refresh: "M20 7v5h-5 M4 17v-5h5 M6 7a7 7 0 0 1 11-2l3 3 M4 16l3 3a7 7 0 0 0 11-2",
  check: "m5 12 4 4L19 6",
  update: "M12 19V5 m-6 6 6-6 6 6",
  warning: "M12 4 2.5 20h19L12 4Z M12 10v4 M12 17h.01",
  panel: "M3 5h18v14H3z M15 5v14",
  close: "m6 6 12 12 M18 6 6 18",
  download: "M12 4v11 M7 10l5 5 5-5 M5 20h14",
  comment: "M21 11.5a8.5 8.5 0 0 1-12.3 7.6L3 20.5l1.4-5A8.5 8.5 0 1 1 21 11.5Z",
  follow: "M5 11a8 8 0 0 1 8 8 M5 5a14 14 0 0 1 14 14 M6 19h.01",
  published: "M7 19a4.5 4.5 0 0 1-.6-8.96 6 6 0 0 1 11.47 1.5A3.75 3.75 0 0 1 17.25 19H7Z M12 16v-5 M9.5 13.5 12 11l2.5 2.5",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  // Actions. One action, one icon: reuse these rather than adding look-alikes.
  plus: "M12 5v14 M5 12h14",
  save: "M5 3h11l3 3v13a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V3Z M8 3v5h7V3 M8 21v-7h8v7",
  trash: "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13 M10 11v6 M14 11v6",
  edit: "M4 20h4L19 9l-4-4L4 16v4Z M13.5 6.5l4 4",
  external: "M14 4h6v6 M20 4l-9 9 M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
  copy: "M9 9h11v11H9z M5 15H4V4h11v1",
  import: "M14 3H6v18h12V7l-4-4Z M14 3v4h4 M12 10v7 M9 14l3 3 3-3",
  export: "M14 3H6v18h12V7l-4-4Z M14 3v4h4 M12 17v-7 M9 13l3-3 3 3",
  undo: "M9 14 4 9l5-5 M4 9h11a5 5 0 0 1 0 10h-3",
  reset: "M3 12a9 9 0 1 0 2.6-6.4L3 8 M3 3v5h5",
  toggleOn: "M8 6h8a6 6 0 0 1 0 12H8A6 6 0 0 1 8 6Z M18.5 12a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z",
  toggleOff: "M8 6h8a6 6 0 0 1 0 12H8A6 6 0 0 1 8 6Z M10.5 12a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z",
  star: "m12 3 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9L12 3Z",
  eye: "M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
  hide: "M3 3l18 18 M10.6 5.6c.5-.1.9-.1 1.4-.1 6 0 9.5 6.5 9.5 6.5a17 17 0 0 1-2.4 3.2 M6.6 6.6C3.9 8.3 2.5 12 2.5 12S6 18.5 12 18.5c1.9 0 3.6-.6 5-1.5 M9.9 9.9a3 3 0 0 0 4.2 4.2",
  unpublish: "M7 19a4.5 4.5 0 0 1-.6-8.96 6 6 0 0 1 11.47 1.5A3.75 3.75 0 0 1 17.25 19H7Z M10 12l4 4 M14 12l-4 4",
  unfollow: "M5 11a8 8 0 0 1 8 8 M5 5a14 14 0 0 1 14 14 M6 19h.01 M3 3l18 18",
  stop: "M7 7h10v10H7z",
  swap: "M7 4 3 8l4 4 M3 8h14 M17 12l4 4-4 4 M21 16H7",
  load: "M3 12h11 M10 8l4 4-4 4 M14 4h5a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-5",
  goTo: "M5 12h14 M13 6l6 6-6 6",
  backup: "M3 5h18v4H3z M5 9v10h14V9 M10 13h4",
  chevronLeft: "m15 6-6 6 6 6",
  chevronRight: "m9 6 6 6-6 6",
  chevronDown: "m6 9 6 6 6-6",
  chevronUp: "m6 15 6-6 6 6",
  flag: "M5 21V4 M5 4h12l-2.5 4L17 12H5",
  send: "M21 3 10 14 M21 3l-6.5 18-4.5-7-7-4.5L21 3Z",
  globe: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z M3 12h18 M12 3c2.5 2.5 3.5 5.5 3.5 9s-1 6.5-3.5 9 M12 3C9.5 5.5 8.5 8.5 8.5 12s1 6.5 3.5 9",
  eraser: "M20 20H9 M4.5 15.5 14 6a2 2 0 0 1 2.8 0l2.2 2.2a2 2 0 0 1 0 2.8L11 19H8l-3.5-3.5Z M9 11l5 5",
  detect: "M19 12a7 7 0 1 1-14 0 7 7 0 0 1 14 0Z M12 2v4 M12 18v4 M2 12h4 M18 12h4",
  skip: "M5 5l10 7-10 7V5Z M19 5v14",
  wrench: "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.8-3.8a6 6 0 0 1-7.9 7.9l-6.9 6.9a2.1 2.1 0 0 1-3-3l6.9-6.9a6 6 0 0 1 7.9-7.9l-3.8 3.8Z",
  bellOff: "M8.7 3.7A6 6 0 0 1 18 9c0 3 .6 5 1.3 6.3 M17 17H4s2-2 2-8c0-.6.1-1.2.3-1.7 M10.3 21a2 2 0 0 0 3.4 0 M3 3l18 18",
  offline: "M3 3l18 18 M8.5 16.4a5 5 0 0 1 7 0 M5 12.9a10 10 0 0 1 4.4-2.6 M19 12.9a10 10 0 0 0-1.9-1.4 M2 8.8a15 15 0 0 1 3.3-2.2 M22 8.8a15 15 0 0 0-11.2-3.7 M12 20h.01",
  code: "m8 8-4 4 4 4 M16 8l4 4-4 4 M14 5l-4 14",
  info: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z M12 11v6 M12 7.5h.01",
  alert: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z M12 7v6 M12 16.5h.01",
};

export type IconName = keyof typeof paths;

/** A stroke icon from the app's own set. `filled` also fills the shape (a set favourite star, say). */
export function Icon({ name, size = 18, filled = false }: { name: IconName; size?: number; filled?: boolean }) {
  return (
    <svg className="ui-icon" width={size} height={size} viewBox="0 0 24 24" fill={filled ? "currentColor" : "none"}
      stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={paths[name]} />
    </svg>
  );
}

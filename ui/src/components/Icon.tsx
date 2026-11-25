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
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
};

export type IconName = keyof typeof paths;

export function Icon({ name }: { name: IconName }) {
  return (
    <svg className="ui-icon" width="18" height="18" viewBox="0 0 24 24" fill="none"
      stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={paths[name]} />
    </svg>
  );
}

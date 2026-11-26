import { toast } from "../../components/Toast";
import { useLayoutPreference } from "../../components/SplitWorkspace";
import type { ApplyReport, Os } from "../../bindings";
import type { QuickSwitchPreference } from "./modlistView";

/** The Quick switch setting (Settings → Modlists), shared by every page that loads modlists. */
export function useQuickSwitch() {
  const [preference, setPreference] = useLayoutPreference<QuickSwitchPreference>("modlist-quick-switch", "review");
  // The config choice used when Quick switch is turned on again (in Settings).
  const [, setLastConfigs] = useLayoutPreference<QuickSwitchPreference>("modlist-quick-switch-configs", "keep-configs");
  const enabled = preference === "keep-configs" || preference === "use-list-configs";
  const remember = (p: QuickSwitchPreference) => {
    if (p !== "review") setLastConfigs(p);
    setPreference(p);
  };
  return { enabled, preference, remember };
}

const names = (items: string[], max = 5) =>
  items.length <= max ? items.join(", ") : `${items.slice(0, max).join(", ")} and ${items.length - max} more`;

/** A one-line notice for a finished Quick switch, naming anything that didn't load. */
export function quickSwitchNotice(title: string, report: ApplyReport | null, os: Os): { text: string; problems: boolean } {
  if (!report) return { text: `Your mods already match "${title}".`, problems: false };
  const parts = [`Switched to "${title}".`];
  if (report.failures.length) parts.push(`Couldn't load: ${names(report.failures)}.`);
  if (report.skipped.length) parts.push(`Left as they are: ${names(report.skipped)}.`);
  const trashed = report.removed - report.toLibrary;
  if (trashed > 0) parts.push(`Moved ${trashed} mod${trashed === 1 ? "" : "s"} to the ${os === "windows" ? "Recycle Bin" : "Trash"}.`);
  return { text: parts.join(" "), problems: parts.length > 1 };
}

/** Shows a finished Quick switch: a warning that stays until closed when something didn't load. */
export function showQuickSwitchToast(notice: { text: string; problems: boolean }) {
  if (notice.problems) toast.warning(notice.text, { id: "quick-switch", title: "Quick switch finished with problems", duration: null });
  else toast.success(notice.text, { id: "quick-switch" });
}

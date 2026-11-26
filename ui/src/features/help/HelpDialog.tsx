import { useOpenUrl } from "../../api";
import { Modal } from "../modlists/Modal";
import { Icon } from "../../components/Icon";
import { useErrorToast } from "../../components/Toast";

/** Same links as 2.3's Help menu. */
export const DISCORD_URL = "https://discord.gg/Zhm3QnD2s9";
export const MANAGER_MODDB_URL = "https://mods.vintagestory.at/simplevsmanager";

/** The Discord logo (filled, unlike the app's stroke icons). */
function DiscordLogo({ size = 56 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M20.32 4.37a19.8 19.8 0 0 0-4.89-1.52.07.07 0 0 0-.08.04c-.21.38-.44.87-.61 1.25a18.3 18.3 0 0 0-5.49 0 12.6 12.6 0 0 0-.62-1.25.08.08 0 0 0-.08-.04 19.7 19.7 0 0 0-4.88 1.52.07.07 0 0 0-.03.03C.53 9.05-.32 13.58.1 18.06a.08.08 0 0 0 .03.06 19.9 19.9 0 0 0 5.99 3.03.08.08 0 0 0 .08-.03c.46-.63.87-1.29 1.23-1.99a.08.08 0 0 0-.04-.11 13.1 13.1 0 0 1-1.87-.89.08.08 0 0 1-.01-.13l.37-.29a.07.07 0 0 1 .08-.01c3.93 1.79 8.18 1.79 12.07 0a.07.07 0 0 1 .08.01l.37.29a.08.08 0 0 1-.01.13c-.6.35-1.22.65-1.87.89a.08.08 0 0 0-.04.11c.36.7.78 1.36 1.23 1.99a.08.08 0 0 0 .08.03 19.8 19.8 0 0 0 6-3.03.08.08 0 0 0 .03-.06c.5-5.18-.84-9.67-3.55-13.66a.06.06 0 0 0-.03-.03ZM8.02 15.33c-1.18 0-2.16-1.09-2.16-2.42 0-1.33.96-2.42 2.16-2.42 1.21 0 2.18 1.1 2.16 2.42 0 1.33-.96 2.42-2.16 2.42Zm7.97 0c-1.18 0-2.15-1.09-2.15-2.42 0-1.33.95-2.42 2.15-2.42 1.21 0 2.18 1.1 2.16 2.42 0 1.33-.95 2.42-2.16 2.42Z" />
    </svg>
  );
}

/** Just the two places to get help: the Discord and the manager's Mod DB page. */
export function HelpDialog({ onClose }: { onClose: () => void }) {
  const openUrl = useOpenUrl();
  useErrorToast(openUrl.error);
  return (
    <Modal title="Help" onClose={onClose}>
      <div className="help">
        <button type="button" className="help-discord" onClick={() => openUrl.mutate(DISCORD_URL)}
          title={DISCORD_URL} aria-label="Join the Discord">
          <DiscordLogo />
        </button>
        <p>If you have any issues or just want to hang out, come say hi on the Discord!</p>
        <button type="button" className="link-button" onClick={() => openUrl.mutate(MANAGER_MODDB_URL)} title={MANAGER_MODDB_URL}>
          <Icon name="external" />Simple VS Manager on the Mod DB
        </button>
      </div>
      <div className="modal-actions">
        <span className="header-spacer" />
        <button type="button" className="primary" onClick={onClose}><Icon name="close" />Close</button>
      </div>
    </Modal>
  );
}

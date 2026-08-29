import { tabTitle, type TabMeta } from "../lib/tabs";
import { panelVariables } from "../theme/panel";
import type { Theme } from "../theme/types";
import "./tabBar.css";

export interface TabBarProps {
  tabs: TabMeta[];
  activeId: string;
  theme: Theme;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onNew: () => void;
}

/**
 * The open drawings.
 *
 * Shown even when there is only one, because on GNOME/Wayland the titlebar
 * never updates (see PROGRESS.md) — this row is the only place the file name
 * and the unsaved marker actually appear.
 */
export default function TabBar({ tabs, activeId, theme, onSelect, onClose, onNew }: TabBarProps) {
  return (
    <div className="tab-bar" style={panelVariables(theme)} role="tablist">
      <div className="tabs">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            role="tab"
            tabIndex={-1}
            aria-selected={tab.id === activeId}
            className={`tab${tab.id === activeId ? " active" : ""}`}
            title={tab.path ?? "Unsaved drawing"}
            onClick={() => onSelect(tab.id)}
            onAuxClick={(event) => {
              if (event.button !== 1) return;
              event.preventDefault();
              onClose(tab.id);
            }}
          >
            {tab.dirty && (
              <span className="mark" aria-label="unsaved changes">
                •
              </span>
            )}
            <span className="title">{tabTitle(tab)}</span>
            <button
              className="close"
              aria-label={`Close ${tabTitle(tab)}`}
              title="Close tab (Ctrl+W)"
              onClick={(event) => {
                // The tab underneath would otherwise select what we just closed.
                event.stopPropagation();
                onClose(tab.id);
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <button className="new" onClick={onNew} title="New tab (Ctrl+T)" aria-label="New tab">
        +
      </button>
    </div>
  );
}

import { useEffect, useRef, useState } from "react";
import { Overlay } from "../ui";

export interface SwitchItem {
  key: string;
  icon: string;
  title: string;
  hint: string;
  open?: boolean;
  color?: string | null;
}

/** ⌃⇥ Switcher: ⌃ 를 누르고 있는 동안 ⇥ 로 이동, ⌃ 를 떼면 선택 */
export function Switcher({ items, start, onPick, onClose }: { items: SwitchItem[]; start: number; onPick: (k: string) => void; onClose: () => void }) {
  const [i, setI] = useState(Math.min(start, items.length - 1));
  const iRef = useRef(i);
  iRef.current = i;
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === "Tab" || e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        const d = (e.key === "Tab" && e.shiftKey) || e.key === "ArrowUp" ? -1 : 1;
        setI((x) => (x + d + items.length) % items.length);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        onPick(items[iRef.current].key);
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Control") onPick(items[iRef.current].key);
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
    };
  }, [items]);
  return (
    <Overlay variant="clear" onClose={onClose}>
      <div className="popup switcher" onMouseDown={(e) => e.stopPropagation()}>
        <div className="popup-title">Switcher</div>
        <div className="popup-list">
          {items.map((it, k) => (
            <div key={it.key} className={"popup-item" + (k === i ? " sel" : "")} onMouseEnter={() => setI(k)} onClick={() => onPick(it.key)}>
              {it.color && <span className="pi-color" style={{ background: it.color }} />}
              <span className="pi-icon">{it.icon}</span>
              <span className="pi-title">{it.title}</span>
              <span className="pi-hint">{it.hint}</span>
            </div>
          ))}
        </div>
        <div className="popup-foot">{items[i]?.hint}</div>
      </div>
    </Overlay>
  );
}

/** ⌘e Recent Files: 입력하면 바로 거른다 (speed search) */
export function RecentFiles({ items, onPick, onClose, title = "Recent Files", start }: { items: SwitchItem[]; onPick: (k: string) => void; onClose: () => void; title?: string; start?: number }) {
  const [q, setQ] = useState("");
  const [i, setI] = useState(start ?? (items.length > 1 ? 1 : 0));
  const shown = q ? items.filter((it) => (it.title + " " + it.hint).toLowerCase().includes(q.toLowerCase())) : items;
  const sel = Math.min(i, Math.max(shown.length - 1, 0));
  return (
    <Overlay variant="clear" onClose={onClose}>
      <div className="popup switcher" onMouseDown={(e) => e.stopPropagation()}>
        <div className="popup-title">{title}</div>
        <input
          className="popup-search"
          autoFocus
          value={q}
          placeholder="Type to search"
          spellCheck={false}
          onChange={(e) => {
            setQ(e.target.value);
            setI(0);
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "ArrowDown") setI((x) => Math.min(x + 1, shown.length - 1));
            else if (e.key === "ArrowUp") setI((x) => Math.max(x - 1, 0));
            else if (e.key === "Enter" && shown[sel]) onPick(shown[sel].key);
            else if (e.key === "Escape") onClose();
            else return;
            e.preventDefault();
          }}
        />
        <div className="popup-list">
          {shown.map((it, k) => (
            <div key={it.key} className={"popup-item" + (k === sel ? " sel" : "") + (it.open ? "" : " closed")} onMouseEnter={() => setI(k)} onClick={() => onPick(it.key)}>
              {it.color && <span className="pi-color" style={{ background: it.color }} />}
              <span className="pi-icon">{it.icon}</span>
              <span className="pi-title">{it.title}</span>
              <span className="pi-hint">{it.hint}</span>
            </div>
          ))}
          {!shown.length && <div className="popup-empty">Nothing found</div>}
        </div>
      </div>
    </Overlay>
  );
}

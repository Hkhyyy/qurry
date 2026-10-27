import { useEffect, useState } from "react";
import { registerTextDialog, TextRequest } from "../lib/dialogs";

/** askText() 가 띄우는 한 줄 입력 창 (Tauri 웹뷰에는 window.prompt 가 없다). App 에 하나 둔다 */
export function TextDialog() {
  const [req, setReq] = useState<TextRequest | null>(null);
  const [value, setValue] = useState("");
  useEffect(() => {
    registerTextDialog((r) => {
      setValue(r.value);
      setReq(r);
    });
    return () => registerTextDialog(null);
  }, []);
  if (!req) return null;
  const done = (v: string | null) => {
    setReq(null);
    req.resolve(v);
  };
  return (
    <div className="modal-backdrop" onMouseDown={() => done(null)}>
      <div className="modal text-dialog" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title">{req.title}</div>
        <input
          autoFocus
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onFocus={(e) => e.target.select()}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") done(value);
            if (e.key === "Escape") done(null);
          }}
        />
        <div className="modal-actions">
          <button onClick={() => done(null)}>Cancel</button>
          <button className="primary" onClick={() => done(value)}>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}

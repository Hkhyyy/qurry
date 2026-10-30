import { useEffect, useState } from "react";
import { registerTextDialog, TextRequest } from "../lib/dialogs";
import { Button, Modal, RadioGroup, TextInput } from "../ui";

/** Single-line input dialog shown by askText() (the Tauri webview has no window.prompt). App holds one */
export function TextDialog() {
  const [req, setReq] = useState<TextRequest | null>(null);
  const [value, setValue] = useState("");
  const [choice, setChoice] = useState(0);
  useEffect(() => {
    registerTextDialog((r) => {
      setValue(r.value);
      setChoice(r.choice ?? 0);
      setReq(r);
    });
    return () => registerTextDialog(null);
  }, []);
  if (!req) return null;
  const done = (v: string | null) => {
    setReq(null);
    req.resolve(v, choice);
  };
  return (
    <Modal
      title={req.title}
      size="sm"
      layer="dialog"
      onClose={() => done(null)}
      footer={
        <>
          <Button onClick={() => done(null)}>Cancel</Button>
          <Button variant="primary" onClick={() => done(value)}>
            OK
          </Button>
        </>
      }
    >
      <TextInput
        full
        autoFocus
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onFocus={(e) => e.target.select()}
        onKeyDown={(e) => {
          if (e.key === "Enter") done(value);
        }}
      />
      {req.choices && (
        <div className="text-dialog-choices">
          <RadioGroup value={choice} onChange={setChoice} options={req.choices.map((c, i) => ({ value: i, label: c }))} />
        </div>
      )}
    </Modal>
  );
}

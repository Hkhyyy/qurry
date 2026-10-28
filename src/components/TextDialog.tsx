import { useEffect, useState } from "react";
import { registerTextDialog, TextRequest } from "../lib/dialogs";
import { Button, Modal, RadioGroup, TextInput } from "../ui";

/** askText() 가 띄우는 한 줄 입력 창 (Tauri 웹뷰에는 window.prompt 가 없다). App 에 하나 둔다 */
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

import { useEffect, useMemo, useState } from "react";
import { registerParamsDialog, type ParamsRequest } from "../lib/dialogs";
import { findParams } from "../lib/params";
import { sharedOr, useSharedReady } from "../lib/shared";
import { Button, CodeBlock, Field, InfoTip, Modal, TextInput, type HlSpan } from "../ui";

/** Characters of the statements shown under the fields */
const PREVIEW = 300;

/** The Parameters dialog (askParams, as DataGrip asks on run): a field per name with its last value, the statements below with the
 *  names marked. Enter in any field runs, Esc cancels */
export function ParamsDialog({ req, onDone }: { req: Omit<ParamsRequest, "resolve">; onDone: (values: Record<string, string> | null) => void }) {
  const [values, setValues] = useState(() => Object.fromEntries(req.names.map((n) => [n, req.last[n] ?? ""])));
  const ready = useSharedReady();
  const shown = req.sql.length > PREVIEW ? req.sql.slice(0, PREVIEW) + "…" : req.sql;
  // The console's colors, with each name marked over them
  const spans = useMemo(() => {
    const names: HlSpan[] = findParams(shown, req.kind).map((p) => ({ start: p.start, end: p.end, class: "param" }));
    const hl = sharedOr<HlSpan[]>([], (s) => s.highlight(shown, req.kind)).filter((h) => !names.some((p) => h.start < p.end && p.start < h.end));
    return [...hl, ...names].sort((a, b) => a.start - b.start);
  }, [shown, req.kind, ready]);
  return (
    <Modal
      title="Parameters"
      layer="dialog"
      onClose={() => onDone(null)}
      headExtra={<InfoTip about="Parameters" info={["A value goes in as you type it, as plain text: type the quotes yourself ('paid', not paid).", "The values are remembered for this console.", "Not a parameter? Cancel, and put the :name in a string or a quoted name."]} />}
      onKeyDown={(e) => {
        // preventDefault: the same Enter must not also press the button focused in a dialog that opens next (a confirmation)
        if (e.key === "Enter" && e.target instanceof HTMLInputElement) {
          e.preventDefault();
          onDone(values);
        }
      }}
      footer={
        <>
          <Button onClick={() => onDone(null)}>Cancel</Button>
          <Button variant="primary" onClick={() => onDone(values)}>
            Run
          </Button>
        </>
      }
    >
      {req.names.map((n, i) => (
        <Field key={n} label={`:${n}`}>
          <TextInput full autoFocus={i === 0} value={values[n]} onChange={(e) => setValues((v) => ({ ...v, [n]: e.target.value }))} onFocus={(e) => e.target.select()} />
        </Field>
      ))}
      <CodeBlock className="params-preview" spans={spans}>
        {shown}
      </CodeBlock>
    </Modal>
  );
}

/** Shows the dialog askParams() asks for. App holds one */
export function ParamsHost() {
  const [req, setReq] = useState<ParamsRequest | null>(null);
  useEffect(() => {
    registerParamsDialog(setReq);
    return () => registerParamsDialog(null);
  }, []);
  if (!req) return null;
  return (
    <ParamsDialog
      req={req}
      onDone={(v) => {
        setReq(null);
        req.resolve(v);
      }}
    />
  );
}

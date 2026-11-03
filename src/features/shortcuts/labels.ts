import { bindings, formatSpec } from "../../lib/shortcuts";

const isMac = navigator.platform.toLowerCase().includes("mac");
/** Shortcut shown in menus and Find Action (the first key) */
const sk = (id: string) => {
  const b = bindings(id);
  return b.length ? formatSpec(b[0]) : undefined;
};

export { isMac, sk };

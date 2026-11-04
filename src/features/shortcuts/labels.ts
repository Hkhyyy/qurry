import { cmd } from "../../lib/shortcuts";

const isMac = navigator.platform.toLowerCase().includes("mac");
/** Shortcut shown in menus and Find Action (the first key) */
const sk = (id: string) => cmd(id).shortcut;

export { isMac, sk };

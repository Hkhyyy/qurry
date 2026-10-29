/** Join classNames (falsy values are dropped) */
export const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

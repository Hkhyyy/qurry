/** Two initials of a workspace name (first letter of each word if several) */
export const initials = (name: string) => {
  const words = name.trim().split(/[\s_\-/]+/).filter(Boolean);
  const s = words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?").slice(0, 2);
  return s.toUpperCase();
};

/** 워크스페이스 이름의 머리글자 두 개 (단어가 여럿이면 각 첫 글자) */
export const initials = (name: string) => {
  const words = name.trim().split(/[\s_\-/]+/).filter(Boolean);
  const s = words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?").slice(0, 2);
  return s.toUpperCase();
};

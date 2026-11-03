import type { StoreApi } from "zustand";

/** A useState-style setter (a value, or a function of the current one) for one field of a store. Applied right away, like the refs it replaces */
export const setter =
  <S, K extends keyof S>(store: StoreApi<S>, key: K) =>
  (v: S[K] | ((cur: S[K]) => S[K])) =>
    store.setState((s) => ({ [key]: typeof v === "function" ? (v as (cur: S[K]) => S[K])(s[key]) : v }) as unknown as Partial<S>);

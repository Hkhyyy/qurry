/**
 * 유기체 (organisms): 분자를 모은 덩어리 — 도구 막대, 대화상자, 확인 창, 메뉴, 팝오버, 목록, 표, 폼 그리드.
 * 규격은 README.md
 */
import { CSSProperties, HTMLAttributes, KeyboardEvent, ReactNode, Ref, TdHTMLAttributes, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button, IconButton, Spacer } from "./atoms";
import { cx } from "./cx";
import { DetailRow, EmptyState, MenuItem, MenuLabel, MenuSeparator, SectionTitle } from "./molecules";

// ───────── 도구 막대 ─────────
export function Toolbar({ children, className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cx("ui-toolbar", className)} role="toolbar" {...rest}>
      {children}
    </div>
  );
}

// ───────── 대화상자 ─────────
export type ModalSize = "sm" | "md" | "lg" | "xl";
/**
 * 대화상자. Esc · 바깥 클릭으로 닫힌다. footer 는 오른쪽 정렬 (취소 → 주 버튼).
 * layer="dialog" 는 다른 모달 위에 뜨는 확인 · 입력 창
 */
export function Modal(p: {
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  size?: ModalSize;
  className?: string;
  layer?: "modal" | "dialog";
  tone?: "danger";
  /** 바깥 클릭으로 닫지 않는다 (입력 중인 폼) */
  stickyBackdrop?: boolean;
  /** 크기 고정: 탭을 바꾸거나 내용이 로딩돼도 창 크기가 그대로 (넘치면 안에서 스크롤) */
  fixed?: boolean;
  headExtra?: ReactNode;
  onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void;
  bodyClassName?: string;
}) {
  // 열리면 포커스를 가져온다 (안의 autoFocus 가 먼저면 그대로) — Esc · Enter 가 늘 이 창으로
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (box.current && !box.current.contains(document.activeElement)) box.current.focus();
  }, []);
  return (
    <div className={cx("ui-backdrop", "for-modal", p.layer === "dialog" && "layer-dialog")} onMouseDown={p.stickyBackdrop ? undefined : p.onClose}>
      <div
        ref={box}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        className={cx("ui-modal", p.size && p.size !== "md" && `size-${p.size}`, p.fixed && "fixed", p.tone && `tone-${p.tone}`, p.className)}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // 앱 전역 단축키로 새지 않게
          e.stopPropagation();
          p.onKeyDown?.(e);
          if (e.key === "Escape" && !e.defaultPrevented) p.onClose();
        }}
      >
        <div className="ui-modal-head">
          <div className="ui-modal-title" title={typeof p.title === "string" ? p.title : undefined}>
            {p.title}
          </div>
          {p.headExtra && <Spacer />}
          {p.headExtra}
        </div>
        <div className={cx("ui-modal-body", p.bodyClassName)}>{p.children}</div>
        {p.footer && <div className="ui-modal-foot">{p.footer}</div>}
      </div>
    </div>
  );
}

/** 팝업 뒤 바탕 (검색 · 전환기 · 빠른 문서). 바깥을 누르면 닫힌다. clear = 어둡게 하지 않음 */
export function Overlay({ onClose, variant, children, onKeyDown }: { onClose: () => void; variant?: "clear" | "light"; children: ReactNode; onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void }) {
  return (
    <div className={cx("ui-backdrop", variant && `overlay-${variant}`)} onMouseDown={onClose} onKeyDown={onKeyDown}>
      {children}
    </div>
  );
}

/** 떠 있는 판 (Search Everywhere · 전환기 · Go to …): 제목 / 머리 · 본문 (목록) · 아래 안내 */
export function Popover({ title, head, foot, children, className, width }: { title?: ReactNode; head?: ReactNode; foot?: ReactNode; children: ReactNode; className?: string; width?: number }) {
  return (
    <div className={cx("ui-popover", className)} style={width ? { width } : undefined} onMouseDown={(e) => e.stopPropagation()}>
      {title && <div className="ui-popover-title">{title}</div>}
      {head}
      {children}
      {foot && <div className="ui-popover-foot">{foot}</div>}
    </div>
  );
}

// ───────── 목록 ─────────
/** 여러 줄 목록 (ListItem 을 담는다). 비면 empty. scroll 영역 · 테두리 있는 상자 (boxed) */
export function List({ children, empty, boxed, className, listRef, style }: { children?: ReactNode; empty?: ReactNode; boxed?: boolean; className?: string; listRef?: Ref<HTMLDivElement>; style?: CSSProperties }) {
  const isEmpty = !children || (Array.isArray(children) && children.flat().filter(Boolean).length === 0);
  return (
    <div className={cx("ui-list", boxed && "boxed", className)} role="listbox" ref={listRef} style={style}>
      {isEmpty && empty ? <EmptyState>{empty}</EmptyState> : children}
    </div>
  );
}
/** 목록 안의 묶음 제목 (Actions · Files · Database) */
export const ListGroup = SectionTitle;

/** 상세 목록 (확인 창의 문장 · 행 수, 세션의 쿼리 · pid …) */
export function DetailList({ items }: { items: { text: ReactNode; value?: ReactNode; tone?: "danger" | "warn" }[] }) {
  return (
    <div className="ui-detail-list">
      {items.map((d, i) => (
        <DetailRow key={i} value={d.value} tone={d.tone}>
          {d.text}
        </DetailRow>
      ))}
    </div>
  );
}

// ───────── 표 (작은 표: 미리 보기 · 매핑 · 값 목록) ─────────
/** 데이터가 적은 표. 큰 결과는 ResultGrid (ui/grid 의 칸 조각) */
export function Table({ children, className }: { children: ReactNode; className?: string }) {
  return <table className={cx("ui-table", className)}>{children}</table>;
}
export const Th = ({ children, num, className, ...rest }: TdHTMLAttributes<HTMLTableCellElement> & { num?: boolean }) => (
  <th className={cx(num && "num", className)} {...rest}>
    {children}
  </th>
);
export const Td = ({ children, num, mono, muted, className, ...rest }: TdHTMLAttributes<HTMLTableCellElement> & { num?: boolean; mono?: boolean; muted?: boolean }) => (
  <td className={cx(num && "num", mono && "mono", muted && "muted", className)} {...rest}>
    {children}
  </td>
);

// ───────── 메뉴 ─────────
export type MenuEntry =
  | { label: string; action: () => void; shortcut?: string; danger?: boolean; disabled?: boolean }
  | { separator: true }
  /** 누를 수 없는 설명 줄 */
  | { heading: string }
  | { label: string; submenu: MenuEntry[] };

/**
 * 메뉴 (우클릭 · ▾ 드롭다운). 바깥 클릭 · ⎋ · 창 포커스 이탈로 닫힌다. 하위 메뉴는 올리면 열린다.
 * at = 화면 좌표 (우클릭) — 없으면 부모 기준 드롭다운. footer = 맨 아래 덧붙임 (색 고르기 등)
 */
export function Menu({ items, onClose, at, footer }: { items: MenuEntry[]; onClose: () => void; at?: { x: number; y: number }; footer?: ReactNode }) {
  const [sub, setSub] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const onDown = (e: globalThis.MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) closeRef.current();
    };
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        closeRef.current();
      }
    };
    const onBlur = () => closeRef.current();
    // 메뉴를 연 클릭이 곧바로 "바깥 클릭"으로 처리되지 않도록 다음 틱에 등록
    const t = setTimeout(() => {
      window.addEventListener("mousedown", onDown, true);
      window.addEventListener("keydown", onKey, true);
      window.addEventListener("blur", onBlur);
    });
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  // 화면 밖으로 나가지 않게
  const style = at ? { left: Math.min(at.x, window.innerWidth - 260), top: Math.max(4, Math.min(at.y, window.innerHeight - 24 * items.length - 40)) } : undefined;
  const run = (fn: () => void) => {
    fn();
    onClose();
  };
  return (
    <div className={cx("ui-menu", !at && "dropdown")} ref={ref} style={style} role="menu" onMouseDown={(e) => e.stopPropagation()}>
      {items.map((it, i) =>
        "separator" in it ? (
          <MenuSeparator key={i} />
        ) : "heading" in it ? (
          <MenuLabel key={i}>{it.heading}</MenuLabel>
        ) : "submenu" in it ? (
          <MenuItem key={i} label={it.label} hasSubmenu onMouseEnter={() => setSub(i)}>
            {sub === i && (
              <div className="ui-menu sub" role="menu">
                {it.submenu.map((s, j) => ("separator" in s ? <MenuSeparator key={j} /> : "heading" in s ? <MenuLabel key={j}>{s.heading}</MenuLabel> : "action" in s ? <MenuItem key={j} label={s.label} shortcut={s.shortcut} danger={s.danger} disabled={s.disabled} onClick={() => run(s.action)} /> : null))}
              </div>
            )}
          </MenuItem>
        ) : (
          <MenuItem key={i} label={it.label} shortcut={it.shortcut} danger={it.danger} disabled={it.disabled} onMouseEnter={() => setSub(null)} onClick={() => run(it.action)} />
        ),
      )}
      {footer && (
        <>
          <MenuSeparator />
          <div className="ui-menu-footer">{footer}</div>
        </>
      )}
    </div>
  );
}

// ───────── 폼 그리드 ─────────
/**
 * 라벨 · 칸이 번갈아 오는 폼 (긴 설정 창). 자식은 <label>라벨</label><칸/> 순서로, 줄 전체는 <FormSection>.
 * 라벨 없는 줄은 빈 <span /> 를 먼저
 */
export function FormGrid({ children, labelWidth = 120, className }: { children: ReactNode; labelWidth?: number; className?: string }) {
  return (
    <div className={cx("ui-form-grid", className)} style={{ ["--ui-form-label" as string]: `${labelWidth}px` }}>
      {children}
    </div>
  );
}

// ───────── confirm() — 앱 안의 확인 창 ─────────
export interface ConfirmOptions {
  title?: string;
  message: ReactNode;
  /** 문장 · 대상과 그 옆 값 (예: 바뀔 행 수) */
  detail?: { text: string; value?: string; tone?: "danger" | "warn" }[];
  okLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}
type ConfirmReq = ConfirmOptions & { resolve: (ok: boolean) => void };
let showConfirm: ((r: ConfirmReq) => void) | null = null;

/** 확인 창을 띄우고 답을 기다린다. ConfirmHost 가 없으면 (테스트) 브라우저 confirm */
export function confirm(o: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    if (showConfirm) showConfirm({ ...o, resolve });
    else resolve(window.confirm(typeof o.message === "string" ? o.message : o.title ?? "") as unknown as boolean);
  });
}

/** App 에 하나 둔다 */
export function ConfirmHost() {
  const [queue, setQueue] = useState<ConfirmReq[]>([]);
  const okRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    showConfirm = (r) => setQueue((q) => [...q, r]);
    return () => {
      showConfirm = null;
    };
  }, []);
  const req = queue[0];
  useEffect(() => {
    // 위험한 확인은 Cancel 에, 나머지는 OK 에 포커스 (Enter 가 무엇을 누르는지 분명하게)
    if (req && !req.danger) okRef.current?.focus();
  }, [req]);
  if (!req) return null;
  const done = (ok: boolean) => {
    setQueue((q) => q.slice(1));
    req.resolve(ok);
  };
  return (
    <Modal
      title={req.title ?? "Qurry"}
      size={req.detail?.length ? "lg" : "sm"}
      layer="dialog"
      tone={req.danger ? "danger" : undefined}
      onClose={() => done(false)}
      footer={
        <>
          <Button autoFocus={req.danger} onClick={() => done(false)}>
            {req.cancelLabel ?? "Cancel"}
          </Button>
          <Button ref={okRef} variant={req.danger ? "danger" : "primary"} onClick={() => done(true)}>
            {req.okLabel ?? "OK"}
          </Button>
        </>
      }
    >
      <div className="ui-confirm-message">{req.message}</div>
      {req.detail && req.detail.length > 0 && <DetailList items={req.detail} />}
    </Modal>
  );
}

// ───────── 탭 줄 ─────────
/**
 * 탭 줄 (편집기 · 결과): 스크롤바 없이 넘친다. 휠 (세로) 로도 옆으로 넘기고, 활성 탭이 바뀌면 보이게 스크롤한다.
 * 넘치면 오른쪽 끝 ▾ — 모든 탭 목록 (DataGrip 의 Show Hidden Tabs). 끌어 놓기 이벤트는 그대로 넘긴다
 */
export function TabStrip({ children, activeKey, overflow, className, ...rest }: HTMLAttributes<HTMLDivElement> & { activeKey?: string | null; overflow?: { key: string; label: string; active?: boolean; onSelect: () => void }[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const [over, setOver] = useState(false);
  const [menu, setMenu] = useState(false);
  // 넘치는지 (▾ 를 보일지)
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const check = () => setOver(el.scrollWidth > el.clientWidth + 1);
    check();
    const ro = new ResizeObserver(check);
    ro.observe(el);
    return () => ro.disconnect();
  });
  // 활성 탭이 보이게
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>(".active")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeKey]);
  return (
    <div className={cx("ui-tab-strip", className)} {...rest}>
      <div
        className="ui-tab-strip-scroll"
        ref={ref}
        role="tablist"
        onWheel={(e) => {
          // 트랙패드 가로 이동은 그대로, 마우스 휠 (세로) 은 옆으로
          if (ref.current && Math.abs(e.deltaY) > Math.abs(e.deltaX)) ref.current.scrollLeft += e.deltaY;
        }}
      >
        {children}
      </div>
      {over && overflow && (
        <span className="ui-tab-strip-more">
          <IconButton size="sm" title="Show All Tabs" onMouseDown={(e) => e.stopPropagation()} onClick={() => setMenu((m) => !m)}>
            ▾
          </IconButton>
          {menu && <Menu onClose={() => setMenu(false)} items={overflow.map((t) => ({ label: (t.active ? "● " : "　") + t.label, action: t.onSelect }))} />}
        </span>
      )}
    </div>
  );
}

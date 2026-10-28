/** 공통 컴포넌트 — 규격은 README.md. 화면은 이것만 쓴다 */
import { ButtonHTMLAttributes, forwardRef, HTMLAttributes, InputHTMLAttributes, KeyboardEvent, ReactNode, SelectHTMLAttributes, useEffect, useRef, useState } from "react";
import "./tokens.css";
import "./ui.css";

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

// ───────── Button ─────────
export type ButtonVariant = "default" | "primary" | "danger" | "ghost" | "link";
export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm" | "md" }>(function Button({ variant = "default", size = "md", className, type = "button", ...rest }, ref) {
  return <button ref={ref} type={type} className={cx("ui-btn", `ui-btn-${variant}`, size === "sm" && "ui-btn-sm", className)} {...rest} />;
});

/** 아이콘만 있는 버튼. title 은 필수 (이름 · 단축키를 보여 준다) */
export function IconButton({ title, active, tone, size = "md", className, type = "button", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { title: string; active?: boolean; tone?: "ok" | "danger"; size?: "sm" | "md" }) {
  return <button type={type} title={title} aria-label={title} className={cx("ui-icon-btn", size === "sm" && "ui-icon-btn-sm", active && "active", tone && `tone-${tone}`, className)} {...rest} />;
}

// ───────── Toolbar ─────────
export function Toolbar({ children, className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cx("ui-toolbar", className)} {...rest}>
      {children}
    </div>
  );
}
/** 도구 막대의 글자 버튼 (Explain ▾, Tx: Auto ▾, Commit …). active = 켜져 있음, accent = 지금 누를 만함 */
export function ToolButton({ active, accent, className, type = "button", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { active?: boolean; accent?: boolean }) {
  return <button type={type} className={cx("ui-tool-btn", active && "active", accent && "accent", className)} {...rest} />;
}
export const ToolbarSep = () => <span className="ui-toolbar-sep" />;
export const Spacer = () => <span className="ui-spacer" />;

// ───────── Badge ─────────
export function Badge({ children, tone = "neutral", title, onClick, className }: { children: ReactNode; tone?: "neutral" | "accent" | "warn" | "danger"; title?: string; onClick?: (e: React.MouseEvent) => void; className?: string }) {
  return (
    <span className={cx("ui-badge", tone !== "neutral" && `tone-${tone}`, onClick && "clickable", className)} title={title} onClick={onClick} onMouseDown={onClick ? (e) => e.stopPropagation() : undefined}>
      {children}
    </span>
  );
}

// ───────── Modal ─────────
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
    <div className={cx("ui-backdrop", p.layer === "dialog" && "layer-dialog")} onMouseDown={p.stickyBackdrop ? undefined : p.onClose}>
      <div
        ref={box}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        className={cx("ui-modal", p.size && p.size !== "md" && `size-${p.size}`, p.tone && `tone-${p.tone}`, p.className)}
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

// ───────── Form ─────────
export function Field({ label, hint, error, children, className }: { label: ReactNode; hint?: ReactNode; error?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={cx("ui-field", className)}>
      <span className="ui-field-label" title={typeof label === "string" ? label : undefined}>
        {label}
      </span>
      {children}
      {hint && <span className="ui-field-hint">{hint}</span>}
      {error && <span className="ui-field-error">{error}</span>}
    </label>
  );
}
export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { full?: boolean }>(function TextInput({ className, full, ...rest }, ref) {
  return <input ref={ref} className={cx("ui-input", full && "full", className)} {...rest} />;
});
export function Select({ className, full, ...rest }: SelectHTMLAttributes<HTMLSelectElement> & { full?: boolean }) {
  return <select className={cx("ui-select", full && "full", className)} {...rest} />;
}
export function Checkbox({ checked, onChange, children, disabled, title }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode; disabled?: boolean; title?: string }) {
  return (
    <label className="ui-check" title={title}>
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} /> {children}
    </label>
  );
}
export function RadioGroup<T extends string | number>({ value, onChange, options, row }: { value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; hint?: ReactNode }[]; row?: boolean }) {
  return (
    <div className={cx("ui-radio-group", row && "row")} role="radiogroup">
      {options.map((o) => (
        <label key={String(o.value)} className="ui-check">
          <input type="radio" checked={value === o.value} onChange={() => onChange(o.value)} /> {o.label}
          {o.hint && <span className="muted"> {o.hint}</span>}
        </label>
      ))}
    </div>
  );
}

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
export const FormSection = ({ children }: { children: ReactNode }) => <div className="ui-form-section">{children}</div>;
/** 칸 아래 · 창 아래의 작은 회색 설명 */
export const Hint = ({ children, className }: { children: ReactNode; className?: string }) => <div className={cx("ui-hint", className)}>{children}</div>;

// ───────── Tabs ─────────
export function Tabs<T extends string>({ items, value, onChange }: { items: { id: T; label: ReactNode }[]; value: T; onChange: (id: T) => void }) {
  return (
    <div className="ui-tabs" role="tablist">
      {items.map((t) => (
        <button key={t.id} type="button" role="tab" aria-selected={value === t.id} className={cx("ui-tab", value === t.id && "active")} onClick={() => onChange(t.id)}>
          {t.label}
        </button>
      ))}
    </div>
  );
}

// ───────── Notice / EmptyState ─────────
export function Notice({ tone, children, className }: { tone?: "warn" | "danger" | "ok"; children: ReactNode; className?: string }) {
  return <div className={cx("ui-notice", tone && `tone-${tone}`, className)}>{children}</div>;
}
export function EmptyState({ title, children, action }: { title?: ReactNode; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="ui-empty">
      {title && <div className="ui-empty-title">{title}</div>}
      {children}
      {action && <div>{action}</div>}
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
      {req.detail && req.detail.length > 0 && (
        <div className="ui-confirm-detail">
          {req.detail.map((d, i) => (
            <div key={i} className="ui-confirm-row">
              <code>{d.text}</code>
              {d.value && <span className={cx("value", d.tone && `tone-${d.tone}`)}>{d.value}</span>}
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}

import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { sharedOr, useSharedReady } from "../../lib/shared";
import { TOKENS, type Palette, type ThemeWarning } from "../../lib/theme";
import { ANCHORS, backdropStyle, CORNERS, FILLS, opacityCap, SIZE_MAX, SIZE_MIN, stickerStyle, stillFrame, warningsOver, type Fill, type Mode, type ThemeBackground } from "../../lib/themeBackground";
import { Button, cx, Named, RadioGroup, Select } from "../../ui";

// Settings › Appearance › Theme › Edit Colors… › Background image (#202, a mockup until it goes into the editor). Two ways to show it:
// Behind (ThemeBackdrop), faint behind the SQL editor and the result grid, drawn as it is (DataGrip's), with the colors that fall below
// their contrast minimum over it counted, never changed; Corner (ThemeSticker), scaled down in front of the window, under every menu,
// dialog and popup, clicks passing through. GIF and animated WebP play in both and stop on their first frame while motion is paused
// (useImageSrc). The TUI ignores it

const LABEL = Object.fromEntries(TOKENS.map((t) => [t.key, t.label.split(",")[0]]));
const POS_LABEL = (a: string) => (a === "center" ? "Center" : a.replace(/^(\w+) (\w+)$/, "$2 $1").replace("center", "middle").replace(/^\w/, (c) => c.toUpperCase()));

/** Motion is paused while the system's Reduce motion is on or the window isn't the active one */
function useMotionPaused(): boolean {
  const media = useMemo(() => window.matchMedia("(prefers-reduced-motion: reduce)"), []);
  const [paused, setPaused] = useState(() => media.matches || !document.hasFocus());
  useEffect(() => {
    const update = () => setPaused(media.matches || !document.hasFocus());
    media.addEventListener("change", update);
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    return () => {
      media.removeEventListener("change", update);
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
    };
  }, [media]);
  return paused;
}

/** The URL to draw: src, or its first frame as a still while motion is paused (or still is set). A still image is unchanged by it */
function useImageSrc(src: string, still?: boolean): string {
  const paused = useMotionPaused() || !!still;
  const [frame, setFrame] = useState<string | null>(null);
  useEffect(() => {
    setFrame(null);
    if (!paused) return;
    let live = true;
    stillFrame(src).then((f) => live && setFrame(f), () => {});
    return () => void (live = false);
  }, [src, paused]);
  // Until the still is ready nothing is drawn, so an animation never plays for a moment
  return paused ? frame ?? "" : src;
}

type Shown = { image: ThemeBackground | null; src: string; /** The story's Reduce motion: the first frame */ still?: boolean };

/** Behind: a surface the image is drawn behind: Background, the image layer, then the children (with clear surfaces) over it */
export function ThemeBackdrop({ image, src, still, className, style, children }: Shown & { className?: string; style?: CSSProperties; children?: ReactNode }) {
  const url = useImageSrc(src, still);
  const on = image?.mode === "behind" && url;
  return (
    <div className={cx("theme-backdrop-host", className)} style={style}>
      {on && <div className="theme-backdrop" aria-hidden style={backdropStyle(image.behind, url)} />}
      {children}
    </div>
  );
}

/** Corner: the image scaled down in a corner, in front of the window and under menus, dialogs, toasts and popups; clicks pass through.
 *  inPreview: in the editor's preview, sized by it */
export function ThemeSticker({ image, src, still, inPreview }: Shown & { inPreview?: boolean }) {
  const url = useImageSrc(src, still);
  if (image?.mode !== "corner" || !url) return null;
  return <img className={cx("theme-sticker", inPreview && "in-preview")} src={url} alt="" aria-hidden draggable={false} style={inPreview ? stickerStyle(image.corner, 6, "%") : stickerStyle(image.corner)} />;
}

/** Placement or corner: a grid of positions as a radio group; the arrow keys move in two dimensions */
function PositionPicker<T extends string>({ label, options, cols, value, disabled, onChange }: { label: string; options: readonly T[]; cols: number; value: T; disabled?: boolean; onChange: (v: T) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const rows = options.length / cols;
  const key = (e: KeyboardEvent) => {
    const i = options.indexOf(value);
    const [r, c] = [Math.floor(i / cols), i % cols];
    const to = { ArrowLeft: [r, c - 1], ArrowRight: [r, c + 1], ArrowUp: [r - 1, c], ArrowDown: [r + 1, c] }[e.key];
    if (!to || to[0] < 0 || to[0] >= rows || to[1] < 0 || to[1] >= cols) return;
    e.preventDefault();
    onChange(options[to[0] * cols + to[1]]);
    (ref.current?.children[to[0] * cols + to[1]] as HTMLElement | undefined)?.focus();
  };
  return (
    <div ref={ref} className="theme-bg-pos" style={{ gridTemplateColumns: `repeat(${cols}, 16px)` }} role="radiogroup" aria-label={label} aria-disabled={disabled} onKeyDown={key}>
      {options.map((a) => (
        <button key={a} type="button" role="radio" aria-checked={a === value} aria-label={POS_LABEL(a)} title={POS_LABEL(a)} tabIndex={a === value ? 0 : -1} disabled={disabled} onClick={() => onChange(a)} />
      ))}
    </div>
  );
}

/** The image as it will be drawn over Background: behind (fill, placement, opacity) or as a corner sticker; "None" without one */
function Thumb({ image, src, p }: { image: ThemeBackground | null; src: string; p: Palette }) {
  return (
    <ThemeBackdrop image={image} src={src} className="theme-bg-thumb" style={{ background: p.bg, color: p.muted }}>
      {!image && <span>None</span>}
      <ThemeSticker image={image} src={src} inPreview />
    </ThemeBackdrop>
  );
}

function Slider({ label, value, min = 0, max = 100, mark, disabled, onChange }: { label: string; value: number; min?: number; max?: number; mark?: number; disabled?: boolean; onChange: (v: number) => void }) {
  return (
    <div className="theme-bg-opacity">
      <span className="theme-bg-range">
        <input type="range" aria-label={label} min={min} max={max} step={1} value={value} disabled={disabled} aria-valuetext={`${value}%`} onChange={(e) => onChange(+e.target.value)} />
        {mark !== undefined && <span className="theme-bg-mark" style={{ left: `calc(7px + (100% - 14px) * ${mark / 100})` }} />}
      </span>
      <span className="theme-bg-pct">{disabled ? "—" : `${value}%`}</span>
    </div>
  );
}

/** value: undefined without an image (off) */
const FillSelect = ({ value, onChange }: { value?: Fill; onChange: (f: Fill) => void }) => (
  <Select aria-label="Fill" value={value ?? "cover"} disabled={!value} onChange={(e) => onChange(e.target.value as Fill)}>
    {FILLS.map((f) => (
      <option key={f.id} value={f.id}>
        {f.label}
      </option>
    ))}
  </Select>
);

/** Held (pointer down, or Space on it), Settings fades so the window behind it, drawing the image live, shows */
function PeekButton() {
  const [on, setOn] = useState(false);
  useEffect(() => {
    if (!on) return;
    const root = document.documentElement;
    root.classList.add("theme-peek");
    const off = () => setOn(false);
    window.addEventListener("pointerup", off);
    window.addEventListener("pointercancel", off);
    window.addEventListener("blur", off);
    return () => {
      root.classList.remove("theme-peek");
      window.removeEventListener("pointerup", off);
      window.removeEventListener("pointercancel", off);
      window.removeEventListener("blur", off);
    };
  }, [on]);
  return (
    <Button
      size="sm"
      className="theme-bg-peek"
      aria-pressed={on}
      onPointerDown={(e) => e.button === 0 && setOn(true)}
      onKeyDown={(e) => e.key === " " && (e.preventDefault(), e.repeat || setOn(true))}
      onKeyUp={(e) => e.key === " " && (e.preventDefault(), setOn(false))}
      onBlur={() => setOn(false)}
    >
      Peek
    </Button>
  );
}

const ABOUT = [
  "PNG, JPEG, WebP or GIF, copied into the config folder and kept with the theme. GIF and animated WebP play, and stop on their first frame while Reduce motion is on or Qurry isn't the active window.",
  "Hold Peek (or Space on it) to see the window behind Settings.",
  "The TUI ignores it: a terminal's own background image shows there.",
];
const SHOW = ["Behind: faint behind the SQL editor and the result grid.", "Corner: scaled down in front of the window, under menus, dialogs and popups; clicks pass through it."];
const FIRST = "Choose an image first.";

/**
 * Settings › Theme's background image section, over the preview: the image (Choose…, Remove), Behind | Corner and each mode's settings
 * (they keep their own), and Peek. Behind's slider marks the opacity up to which every color keeps its minimum over the image's lightest
 * and darkest pixels; above it the colors that fall below are counted (and listed in Opacity's ⓘ), saved with their warning
 */
export function BackgroundImageSection({ image, src, p, onPick, onChange }: { image: ThemeBackground | null; src: string; p: Palette; /** Opens the file picker, copies the file and sets image */ onPick: () => void; onChange: (image: ThemeBackground | null) => void }) {
  const ready = useSharedReady();
  const key = JSON.stringify(p);
  const cap = useMemo(() => sharedOr(0, (s) => opacityCap(p, s.themeWarnings)), [key, ready]);
  const opacity = image?.behind.opacity;
  const below = useMemo(() => (image?.mode === "behind" ? sharedOr<ThemeWarning[]>([], (s) => warningsOver(p, opacity!, s.themeWarnings)) : []), [key, image?.mode, opacity, ready]);
  const mode: Mode = image?.mode ?? "behind";
  const behind = (v: Partial<ThemeBackground["behind"]>) => image && onChange({ ...image, behind: { ...image.behind, ...v } });
  const corner = (v: Partial<ThemeBackground["corner"]>) => image && onChange({ ...image, corner: { ...image.corner, ...v } });
  const first = image ? [] : [FIRST];
  const names = below.map((w) => `${LABEL[w.fg]} on ${LABEL[w.bg]}: ${w.ratio.toFixed(1)}:1, needs ${w.min}:1`);
  return (
    <div className="theme-bg-bar" role="group" aria-label="Background image">
      <div className="theme-bg-bar-image">
        <Thumb image={image} src={src} p={p} />
        <div className="ui-row">
          <Button size="sm" onClick={onPick}>
            Choose…
          </Button>
          <Button size="sm" disabled={!image} onClick={() => onChange(null)}>
            Remove
          </Button>
        </div>
      </div>
      <div className="theme-bg-bar-fields">
        <div className="theme-bg-bar-title">
          <Named about="Background image" info={ABOUT}>
            Background image
          </Named>
          <span className="theme-bg-name muted" title={image?.file}>
            {image?.file ?? "No image"}
          </span>
          <PeekButton />
        </div>
        <div className="theme-bg-row">
          <Named about="Show" info={[...first, ...SHOW]}>
            Show
          </Named>
          <RadioGroup row value={mode} onChange={(m) => image && onChange({ ...image, mode: m })} options={[{ value: "behind" as Mode, label: "Behind", disabled: !image }, { value: "corner" as Mode, label: "Corner", disabled: !image }]} />
        </div>
        {/* Both modes' rows take the same height: switching doesn't move the preview */}
        <div className="theme-bg-mode">
          {mode === "behind" ? (
            <>
              <div className="theme-bg-row">
                <Named about="Opacity" info={[...first, `Up to ${cap}% in this theme, every color keeps its contrast minimum over the image's lightest and darkest pixels (the mark). Above it, the colors that fall below are warned about, never changed.`, ...names]}>
                  Opacity
                </Named>
                <Slider label="Opacity" value={opacity ?? 0} mark={cap} disabled={!image} onChange={(v) => behind({ opacity: v })} />
                <span className="theme-warn theme-bg-below" style={{ visibility: below.length ? "visible" : "hidden" }}>
                  ⚠ {below.length} below
                </span>
              </div>
              <div className="theme-bg-row">
                <Named about="Fill" info={[...first, "Cover fills the area and crops the image; Contain shows all of it; Tile repeats it; Center keeps its size."]}>
                  Fill
                </Named>
                <FillSelect value={image?.behind.fill} onChange={(fill) => behind({ fill })} />
              </div>
              <div className="theme-bg-row">
                <Named about="Placement" info={[...first, "Where the image sits when it doesn't fill the area, and which part stays when Cover crops it."]}>
                  Placement
                </Named>
                <PositionPicker label="Placement" options={ANCHORS} cols={3} value={image?.behind.anchor ?? "center"} disabled={!image} onChange={(anchor) => behind({ anchor })} />
              </div>
            </>
          ) : (
            <>
              <div className="theme-bg-row">
                <Named about="Corner" info={["The window's corner it sits in, 16 px from the edges."]}>
                  Corner
                </Named>
                <PositionPicker label="Corner" options={CORNERS} cols={2} value={image!.corner.corner} onChange={(c) => corner({ corner: c })} />
              </div>
              <div className="theme-bg-row">
                <Named about="Size" info={["Its height, as a share of the window's."]}>
                  Size
                </Named>
                <Slider label="Size" value={image!.corner.size} min={SIZE_MIN} max={SIZE_MAX} onChange={(size) => corner({ size })} />
              </div>
              <div className="theme-bg-row">
                <Named about="Opacity" info={["No contrast warning: it covers what's under it, as placed."]}>
                  Opacity
                </Named>
                <Slider label="Opacity" value={image!.corner.opacity} onChange={(v) => corner({ opacity: v })} />
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

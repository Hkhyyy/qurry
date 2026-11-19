import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { sharedOr, useSharedReady } from "../../lib/shared";
import { TOKENS, type Palette, type ThemeWarning } from "../../lib/theme";
import { ANCHORS, backdropStyle, CORNERS, FILLS, opacityCap, SIZE_MAX, SIZE_MIN, stickerStyle, stillFrame, warningsOver, type BackgroundSlot, type CornerSlot, type Fill, type ThemeImages } from "../../lib/themeBackground";
import { Button, cx, Named, Select } from "../../ui";

// Settings › Appearance › Theme › Edit Colors… › Images (#202, a mockup until it goes into the editor): two slots, each optional with
// its own image, both can be on. Background (ThemeBackdrop): faint behind the SQL editor and the result grid, drawn as it is
// (DataGrip's), with the colors that fall below their contrast minimum over it counted, never changed. Corner (ThemeSticker): scaled
// down in front of the window, under every menu, dialog and popup, clicks passing through. GIF and animated WebP play in each slot and
// stop on their first frame while motion is paused (useImageSrc). The TUI ignores them

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
    if (!paused || !src) return;
    let live = true;
    stillFrame(src).then((f) => live && setFrame(f), () => {});
    return () => void (live = false);
  }, [src, paused]);
  // Until the still is ready nothing is drawn, so an animation never plays for a moment
  return paused ? frame ?? "" : src;
}

/** still: the story's Reduce motion (the first frame) */
type Shown = { src: string; still?: boolean };

/** Background: a surface the image is drawn behind: Background, the image layer, then the children (with clear surfaces) over it */
export function ThemeBackdrop({ slot, src, still, className, style, children }: Shown & { slot?: BackgroundSlot; className?: string; style?: CSSProperties; children?: ReactNode }) {
  const url = useImageSrc(slot ? src : "", still);
  return (
    <div className={cx("theme-backdrop-host", className)} style={style}>
      {slot && url && <div className="theme-backdrop" aria-hidden style={backdropStyle(slot, url)} />}
      {children}
    </div>
  );
}

/** Corner: the image scaled down in a corner, in front of the window and under menus, dialogs, toasts and popups; clicks pass through.
 *  inPreview: in the editor's preview (or a thumbnail), sized by it */
export function ThemeSticker({ slot, src, still, inPreview }: Shown & { slot?: CornerSlot; inPreview?: boolean }) {
  const url = useImageSrc(slot ? src : "", still);
  if (!slot || !url) return null;
  return <img className={cx("theme-sticker", inPreview && "in-preview")} src={url} alt="" aria-hidden draggable={false} style={inPreview ? stickerStyle(slot, 4, "%") : stickerStyle(slot)} />;
}

/** Placement or corner: a grid of positions as a radio group; the arrow keys move in two dimensions */
function PositionPicker<T extends string>({ label, options, cols, value, onChange }: { label: string; options: readonly T[]; cols: number; value: T; onChange: (v: T) => void }) {
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
    <div ref={ref} className="theme-bg-pos" style={{ gridTemplateColumns: `repeat(${cols}, 16px)` }} role="radiogroup" aria-label={label} onKeyDown={key}>
      {options.map((a) => (
        <button key={a} type="button" role="radio" aria-checked={a === value} aria-label={POS_LABEL(a)} title={POS_LABEL(a)} tabIndex={a === value ? 0 : -1} onClick={() => onChange(a)} />
      ))}
    </div>
  );
}

function Slider({ label, value, min = 0, max = 100, mark, onChange }: { label: string; value: number; min?: number; max?: number; mark?: number; onChange: (v: number) => void }) {
  return (
    <div className="theme-bg-opacity">
      <span className="theme-bg-range">
        <input type="range" aria-label={label} min={min} max={max} step={1} value={value} aria-valuetext={`${value}%`} onChange={(e) => onChange(+e.target.value)} />
        {mark !== undefined && <span className="theme-bg-mark" style={{ left: `calc(7px + (100% - 14px) * ${mark / 100})` }} />}
      </span>
      <span className="theme-bg-pct">{value}%</span>
    </div>
  );
}

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

/** A slot: its name, file, Choose… and Remove; with an image, its thumbnail and settings under them */
function Slot({ name, info, file, onPick, onRemove, thumb, children }: { name: string; info: string[]; file?: string; onPick: () => void; onRemove: () => void; thumb: ReactNode; children: ReactNode }) {
  return (
    <div className="theme-bg-slot" role="group" aria-label={name}>
      <div className="theme-bg-slot-head">
        <Named about={name} info={info}>
          {name}
        </Named>
        <span className="theme-bg-name muted" title={file}>
          {file ?? "No image"}
        </span>
        <Button size="sm" aria-label={`Choose ${name} image`} onClick={onPick}>
          Choose…
        </Button>
        {file && (
          <Button size="sm" aria-label={`Remove ${name} image`} onClick={onRemove}>
            Remove
          </Button>
        )}
      </div>
      {file && (
        <div className="theme-bg-slot-body">
          {thumb}
          <div className="theme-bg-fields">{children}</div>
        </div>
      )}
    </div>
  );
}

const ABOUT = [
  "PNG, JPEG, WebP or GIF, each copied into the config folder and kept with the theme: up to 4 MB, 8 MB when animated. GIF and animated WebP play, and stop on their first frame while Reduce motion is on or Qurry isn't the active window.",
  "Hold Peek (or Space on it) to see the window behind Settings.",
  "The TUI ignores them: a terminal's own background image shows there.",
];

/**
 * Settings › Theme's images, over the preview: Background and Corner, each with its own image (Choose…, Remove) and settings, both can
 * be on; Peek. Background's slider marks the opacity up to which every color keeps its minimum over the image's lightest and darkest
 * pixels; above it the colors that fall below are counted (and listed in Opacity's ⓘ), saved with their warning
 */
export function ThemeImagesSection({ images, srcOf, p, onPick, onChange }: { images: ThemeImages; /** A file's image URL */ srcOf: (file: string) => string; p: Palette; /** Opens the file picker for a slot, copies the file and sets it */ onPick: (slot: keyof ThemeImages) => void; onChange: (images: ThemeImages) => void }) {
  const ready = useSharedReady();
  const key = JSON.stringify(p);
  const { background: bg, corner: cn } = images;
  const cap = useMemo(() => sharedOr(0, (s) => opacityCap(p, s.themeWarnings)), [key, ready]);
  const below = useMemo(() => (bg ? sharedOr<ThemeWarning[]>([], (s) => warningsOver(p, bg.opacity, s.themeWarnings)) : []), [key, bg?.opacity, ready]);
  const setBg = (v: Partial<BackgroundSlot>) => bg && onChange({ ...images, background: { ...bg, ...v } });
  const setCn = (v: Partial<CornerSlot>) => cn && onChange({ ...images, corner: { ...cn, ...v } });
  const names = below.map((w) => `${LABEL[w.fg]} on ${LABEL[w.bg]}: ${w.ratio.toFixed(1)}:1, needs ${w.min}:1`);
  const thumb = { className: "theme-bg-thumb", style: { background: p.bg } };
  return (
    <div className="theme-bg-bar" role="group" aria-label="Theme images">
      <div className="theme-bg-bar-title">
        <Named about="Theme images" info={ABOUT}>
          Images
        </Named>
        <PeekButton />
      </div>
      <Slot
        name="Background"
        info={["Faint behind the SQL editor and the result grid."]}
        file={bg?.file}
        onPick={() => onPick("background")}
        onRemove={() => onChange({ ...images, background: undefined })}
        thumb={<ThemeBackdrop slot={bg} src={bg ? srcOf(bg.file) : ""} {...thumb} />}
      >
        {bg && (
          <>
            <div className="theme-bg-row">
              <Named about="Opacity" info={[`Up to ${cap}% in this theme, every color keeps its contrast minimum over the image's lightest and darkest pixels (the mark). Above it, the colors that fall below are warned about, never changed.`, ...names]}>
                Opacity
              </Named>
              <Slider label="Background opacity" value={bg.opacity} mark={cap} onChange={(opacity) => setBg({ opacity })} />
              <span className="theme-warn theme-bg-below" style={{ visibility: below.length ? "visible" : "hidden" }}>
                ⚠ {below.length} below
              </span>
            </div>
            <div className="theme-bg-row">
              <Named about="Fill" info={["Cover fills the area and crops the image; Contain shows all of it; Tile repeats it; Center keeps its size."]}>
                Fill
              </Named>
              <Select aria-label="Fill" value={bg.fill} onChange={(e) => setBg({ fill: e.target.value as Fill })}>
                {FILLS.map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.label}
                  </option>
                ))}
              </Select>
            </div>
            <div className="theme-bg-row">
              <Named about="Placement" info={["Where the image sits when it doesn't fill the area, and which part stays when Cover crops it."]}>
                Placement
              </Named>
              <PositionPicker label="Placement" options={ANCHORS} cols={3} value={bg.anchor} onChange={(anchor) => setBg({ anchor })} />
            </div>
          </>
        )}
      </Slot>
      <Slot
        name="Corner"
        info={["Scaled down in front of the window, 16 px from its edges, under menus, dialogs and popups; clicks pass through it. No contrast warning: it covers what's under it, as placed."]}
        file={cn?.file}
        onPick={() => onPick("corner")}
        onRemove={() => onChange({ ...images, corner: undefined })}
        thumb={
          <div {...thumb}>
            <ThemeSticker slot={cn && { ...cn, size: 80 }} src={cn ? srcOf(cn.file) : ""} inPreview />
          </div>
        }
      >
        {cn && (
          <>
            <div className="theme-bg-row">
              <span className="theme-bg-label">Position</span>
              <PositionPicker label="Position" options={CORNERS} cols={2} value={cn.corner} onChange={(corner) => setCn({ corner })} />
            </div>
            <div className="theme-bg-row">
              <Named about="Size" info={["Its height, as a share of the window's."]}>
                Size
              </Named>
              <Slider label="Corner size" value={cn.size} min={SIZE_MIN} max={SIZE_MAX} onChange={(size) => setCn({ size })} />
            </div>
            <div className="theme-bg-row">
              <span className="theme-bg-label">Opacity</span>
              <Slider label="Corner opacity" value={cn.opacity} onChange={(opacity) => setCn({ opacity })} />
            </div>
          </>
        )}
      </Slot>
    </div>
  );
}

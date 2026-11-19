import { useMemo, useRef, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { sharedOr, useSharedReady } from "../../lib/shared";
import { TOKENS, type Kind, type Palette, type ThemeWarning } from "../../lib/theme";
import { ANCHORS, backdropStyle, canShow, FILLS, opacityCap, warningsOver, type Anchor, type Blend, type Fill, type ThemeBackground } from "../../lib/themeBackground";
import { Button, cx, Named, Select } from "../../ui";

// Settings › Appearance › Theme › Edit Colors… › Background image (#202, a mockup: two options, one to be picked). The image is drawn
// behind the SQL editor and the result grid only (ThemeBackdrop); menus, dialogs and popups keep their own surface. The TUI ignores it.
// Option A (BackgroundImageGroup): rows in the colors column, the image only darkens a dark Background (lightens a light one), so no
// contrast check can fail. Option B (BackgroundImageBar): a bar over the preview, the image drawn as it is (DataGrip's), and the colors
// that fall below their minimum over it are warned about, as the editor warns about colors, never changed

const LABEL = Object.fromEntries(TOKENS.map((t) => [t.key, t.label.split(",")[0]]));
const ANCHOR_LABEL = (a: Anchor) => (a === "center" ? "Center" : a.replace(/^(\w+) (\w+)$/, "$2 $1").replace("center", "middle").replace(/^\w/, (c) => c.toUpperCase()));

/** A surface the image is drawn behind: Background, the image layer, then the children (with transparent surfaces) over it */
export function ThemeBackdrop({ image, src, kind, blend, className, style, children }: { image: ThemeBackground | null; src: string; kind: Kind; blend: Blend; className?: string; style?: CSSProperties; children?: ReactNode }) {
  return (
    <div className={cx("theme-backdrop-host", className)} style={style}>
      {image && <div className="theme-backdrop" aria-hidden style={backdropStyle(image, src, kind, blend)} />}
      {children}
    </div>
  );
}

/** Placement: DataGrip's nine anchors as a 3×3 radio group; the arrow keys move in two dimensions */
function AnchorPicker({ value, disabled, onChange }: { value: Anchor; disabled?: boolean; onChange: (a: Anchor) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const key = (e: KeyboardEvent) => {
    const i = ANCHORS.indexOf(value);
    const [r, c] = [Math.floor(i / 3), i % 3];
    const to = { ArrowLeft: [r, c - 1], ArrowRight: [r, c + 1], ArrowUp: [r - 1, c], ArrowDown: [r + 1, c] }[e.key];
    if (!to || to[0] < 0 || to[0] > 2 || to[1] < 0 || to[1] > 2) return;
    e.preventDefault();
    onChange(ANCHORS[to[0] * 3 + to[1]]);
    (ref.current?.children[to[0] * 3 + to[1]] as HTMLElement | undefined)?.focus();
  };
  return (
    <div ref={ref} className="theme-bg-anchor" role="radiogroup" aria-label="Placement" aria-disabled={disabled} onKeyDown={key}>
      {ANCHORS.map((a) => (
        <button key={a} type="button" role="radio" aria-checked={a === value} aria-label={ANCHOR_LABEL(a)} title={ANCHOR_LABEL(a)} tabIndex={a === value ? 0 : -1} disabled={disabled} onClick={() => onChange(a)} />
      ))}
    </div>
  );
}

/** The image as it will be drawn (fill, placement, opacity, blend) over Background; "None" without one */
function Thumb({ image, src, p, kind, blend, w, h }: { image: ThemeBackground | null; src: string; p: Palette; kind: Kind; blend: Blend; w: number; h: number }) {
  return (
    <ThemeBackdrop image={image} src={src} kind={kind} blend={blend} className="theme-bg-thumb" style={{ width: w, height: h, background: p.bg, color: p.muted }}>
      {!image && <span>None</span>}
    </ThemeBackdrop>
  );
}

function Opacity({ value, max, disabled, onChange, mark }: { value: number; max: number; disabled?: boolean; onChange: (v: number) => void; mark?: number }) {
  return (
    <div className="theme-bg-opacity">
      <span className="theme-bg-range">
        <input type="range" aria-label="Opacity" min={0} max={max} step={1} value={Math.min(value, max)} disabled={disabled} aria-valuetext={`${value}%`} onChange={(e) => onChange(+e.target.value)} />
        {mark !== undefined && <span className="theme-bg-mark" style={{ left: `calc(7px + (100% - 14px) * ${mark / 100})` }} title={`Contrast kept up to ${mark}%`} />}
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

type Props = {
  image: ThemeBackground | null;
  /** The image's URL (the copy in the config folder) */
  src: string;
  p: Palette;
  kind: Kind;
  /** Opens the file picker (PNG, JPEG, WebP), copies the file and sets image */
  onPick: () => void;
  onChange: (image: ThemeBackground | null) => void;
};
const WHERE = "Drawn behind the SQL editor and the result grid; never behind menus, dialogs or popups.";
const FILE = "PNG, JPEG or WebP up to 4 MB, copied into the config folder and kept with the theme.";
const TUI = "The TUI ignores it: a terminal's own background image shows there.";
const FILL_INFO = "Cover fills the area and crops the image; Contain shows all of it; Tile repeats it; Center keeps its size.";
const PLACE_INFO = "Where the image sits when it doesn't fill the area, and which part stays when Cover crops it.";
const FIRST = "Choose an image first.";

/** Option A: rows under Basic colors in the editor's colors column. The image only darkens a dark Background (only lightens a light
 *  one), so every color keeps its contrast at any opacity; on a black (white) Background it has no room and Opacity says so */
export function BackgroundImageGroup({ image, src, p, kind, onPick, onChange }: Props) {
  const show = canShow(p, kind, "one-way");
  const set = (v: Partial<ThemeBackground>) => image && onChange({ ...image, ...v });
  const first = image ? [] : [FIRST];
  const room = kind === "light" ? "Background is white, and the image only lightens it: it can't show. A darker Background gives it room." : "Background is black, and the image only darkens it: it can't show. A lighter Background gives it room.";
  return (
    <div className="theme-bg-group">
      <div className="theme-group">
        <Named about="Background image" info={[WHERE, FILE, TUI]}>
          Background image
        </Named>
      </div>
      <div className="theme-bg-image">
        <Thumb image={image} src={src} p={p} kind={kind} blend="one-way" w={72} h={45} />
        <div className="theme-bg-file">
          <span className="theme-bg-name" title={image?.file}>
            {image?.file ?? "No image"}
          </span>
          <div className="ui-row">
            <Button size="sm" onClick={onPick}>
              Choose…
            </Button>
            <Button size="sm" disabled={!image} onClick={() => onChange(null)}>
              Remove
            </Button>
          </div>
        </div>
      </div>
      <div className="theme-bg-row">
        <Named about="Opacity" info={image && !show ? [room] : [...first, "It only darkens Background (in a light theme, only lightens it), so every color keeps its contrast."]}>
          Opacity
        </Named>
        <Opacity value={image?.opacity ?? 0} max={100} disabled={!image || !show} onChange={(opacity) => set({ opacity })} />
      </div>
      <div className="theme-bg-row">
        <Named about="Fill" info={[...first, FILL_INFO]}>
          Fill
        </Named>
        <FillSelect value={image?.fill} onChange={(fill) => set({ fill })} />
      </div>
      <div className="theme-bg-row">
        <Named about="Placement" info={[...first, PLACE_INFO]}>
          Placement
        </Named>
        <AnchorPicker value={image?.anchor ?? "center"} disabled={!image} onChange={(anchor) => set({ anchor })} />
      </div>
    </div>
  );
}

/** Option B: a bar over the preview, next to what it changes. The image is drawn as it is (DataGrip's, 15% by default); the slider
 *  marks the opacity up to which every color keeps its minimum over the image's lightest and darkest pixels, and above it the colors
 *  that fall below are counted here (and would be in the editor's header), saved with their warning, never changed */
export function BackgroundImageBar({ image, src, p, kind, onPick, onChange }: Props) {
  const ready = useSharedReady();
  const key = JSON.stringify(p) + kind;
  const cap = useMemo(() => sharedOr(0, (s) => opacityCap(p, kind, "both", s.themeWarnings)), [key, ready]);
  const below = useMemo(() => (image ? sharedOr<ThemeWarning[]>([], (s) => warningsOver(p, kind, "both", image.opacity, s.themeWarnings)) : []), [key, image?.opacity, ready]);
  const set = (v: Partial<ThemeBackground>) => image && onChange({ ...image, ...v });
  const first = image ? [] : [FIRST];
  const names = below.map((w) => `${LABEL[w.fg]} on ${LABEL[w.bg]}: ${w.ratio.toFixed(1)}:1, needs ${w.min}:1`);
  return (
    <div className="theme-bg-bar" role="group" aria-label="Background image">
      <div className="theme-bg-bar-image">
        <Thumb image={image} src={src} p={p} kind={kind} blend="both" w={112} h={70} />
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
          <Named about="Background image" info={[WHERE, FILE, TUI]}>
            Background image
          </Named>
          <span className="theme-bg-name muted" title={image?.file}>
            {image?.file ?? "No image"}
          </span>
        </div>
        <div className="theme-bg-row">
          <Named about="Opacity" info={[...first, `Up to ${cap}% in this theme, every color keeps its contrast minimum over the image's lightest and darkest pixels (the mark). Above it, the colors that fall below are warned about, never changed.`, ...names]}>
            Opacity
          </Named>
          <Opacity value={image?.opacity ?? 0} max={100} mark={cap} disabled={!image} onChange={(opacity) => set({ opacity })} />
          <span className="theme-warn theme-bg-below" style={{ visibility: below.length ? "visible" : "hidden" }}>
            ⚠ {below.length} below
          </span>
        </div>
        <div className="theme-bg-row">
          <Named about="Fill" info={[...first, FILL_INFO]}>
            Fill
          </Named>
          <FillSelect value={image?.fill} onChange={(fill) => set({ fill })} />
        </div>
        <div className="theme-bg-row">
          <Named about="Placement" info={[...first, PLACE_INFO]}>
            Placement
          </Named>
          <AnchorPicker value={image?.anchor ?? "center"} disabled={!image} onChange={(anchor) => set({ anchor })} />
        </div>
      </div>
    </div>
  );
}

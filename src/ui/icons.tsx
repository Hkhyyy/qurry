/** Tree icons (hand-drawn 16px SVG — same shapes · colors as DataGrip) */
const S = (p: { children: React.ReactNode; title?: string }) => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
    {p.children}
  </svg>
);

export const Chevron = ({ open }: { open: boolean }) => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
    {open ? (
      <path d="M4.5 6.5 8 10l3.5-3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
    ) : (
      <path d="M6.5 4.5 10 8l-3.5 3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
    )}
  </svg>
);

/** PostgreSQL data source (elephant) */
export const PgIcon = () => (
  <S>
    <path
      d="M5.2 2.6c-1.7 0-2.9 1.3-2.9 3.3 0 1.6.6 3.5 1.5 4.6.5.6 1 .7 1.4.4.3-.2.4-.6.5-1.1l.2-1c.5.3 1 .4 1.6.4.3 0 .6 0 .9-.1v2.4c0 1.1.7 1.9 1.6 1.9.9 0 1.5-.6 1.6-1.7l.2-2.4c.9-.6 1.6-1.6 1.9-2.9.4-1.8-.4-3.4-2-3.9-.9-.3-1.8-.2-2.6.2-.6-.4-1.3-.6-2-.6Z"
      fill="#336791"
      stroke="#dfe7f2"
      strokeWidth=".7"
    />
    <circle cx="10.3" cy="5.6" r=".6" fill="#fff" />
  </S>
);

export const MySqlIcon = () => (
  <S>
    <path d="M2 11c2-1 3.5-4 6-6 1.6-1.3 3.6-1.6 5.5-1-1 .6-1.6 1.5-1.6 2.7 0 1.8-1.4 3.2-3.5 3.6-2.4.4-4.4.4-6.4.7Z" fill="#00758f" />
  </S>
);

export const RedisIcon = () => (
  <S>
    <path d="M2 6l6-3 6 3-6 3-6-3Z" fill="#d82c20" />
    <path d="M2 9l6 3 6-3" stroke="#d82c20" strokeWidth="1.4" />
  </S>
);

/** Database: cylinder (DataGrip New UI) */
export const DatabaseIcon = () => (
  <S>
    <ellipse cx="8" cy="4" rx="5" ry="1.9" stroke="var(--ui-icon-db)" strokeWidth="1.1" />
    <path d="M3 4v8c0 1.05 2.24 1.9 5 1.9s5-.85 5-1.9V4" stroke="var(--ui-icon-db)" strokeWidth="1.1" />
    <path d="M3 8c0 1.05 2.24 1.9 5 1.9s5-.85 5-1.9" stroke="var(--ui-icon-db)" strokeWidth="1.1" />
  </S>
);

/** Schema: stacked layers */
export const SchemaIcon = () => (
  <S>
    <path d="M8 2.5 13.5 5.3 8 8.1 2.5 5.3 8 2.5Z" stroke="var(--ui-icon)" strokeWidth="1.1" strokeLinejoin="round" />
    <path d="M2.5 8.1 8 10.9l5.5-2.8M2.5 10.9 8 13.7l5.5-2.8" stroke="var(--ui-icon)" strokeWidth="1.1" strokeLinejoin="round" />
  </S>
);

/** Group folder (tables · views …): outline only */
export const FolderIcon = () => (
  <S>
    <path d="M2 4.3c0-.6.5-1.1 1.1-1.1h3.1l1.4 1.5h5.3c.6 0 1.1.5 1.1 1.1v6c0 .6-.5 1.1-1.1 1.1H3.1c-.6 0-1.1-.5-1.1-1.1V4.3Z" stroke="var(--ui-icon)" strokeWidth="1.1" strokeLinejoin="round" />
  </S>
);

/** Table: cells + blue header row */
export const TableIcon = () => (
  <S>
    <rect x="2.5" y="3" width="11" height="10" rx="1.2" stroke="var(--ui-icon)" strokeWidth="1.1" />
    <path d="M3 3.6h10V6.3H3Z" fill="var(--ui-icon-db)" opacity=".55" />
    <path d="M2.5 6.5h11M2.5 9.7h11M6.2 6.5V13M9.8 6.5V13" stroke="var(--ui-icon)" strokeWidth=".9" />
  </S>
);

export const ViewIcon = () => (
  <S>
    <rect x="2" y="2.5" width="12" height="11" rx="1.2" stroke="#7fb06e" strokeWidth="1.1" />
    <path d="M2 6.2h12M6 2.5v11" stroke="#7fb06e" strokeWidth="1" />
    <circle cx="10" cy="10" r="1.6" stroke="#7fb06e" strokeWidth="1" />
  </S>
);

export const SequenceIcon = () => (
  <S>
    <text x="1.5" y="11.5" fontSize="7.5" fontFamily="Menlo, monospace" fill="#c9a26d">123</text>
  </S>
);

export const RoutineIcon = () => (
  <S>
    <text x="2" y="12" fontSize="10" fontStyle="italic" fontFamily="Georgia, serif" fill="#56a8f5">ƒ</text>
    <text x="8" y="12" fontSize="7" fontFamily="Menlo, monospace" fill="#56a8f5">()</text>
  </S>
);

export const ColumnIcon = ({ pk, notNull }: { pk?: boolean; notNull?: boolean }) =>
  pk ? (
    <S>
      <circle cx="5.5" cy="8" r="3" stroke="#e5c07b" strokeWidth="1.4" />
      <path d="M8.5 8H14M12 8v2.5M14 8v2" stroke="#e5c07b" strokeWidth="1.4" />
    </S>
  ) : (
    <S>
      <rect x="3" y="5.5" width="10" height="5" rx="1" stroke={notNull ? "#9aa6c4" : "#7d828a"} strokeWidth="1.1" />
      <rect x="4.5" y="7" width="3" height="2" fill={notNull ? "#9aa6c4" : "#7d828a"} />
    </S>
  );

export const KeyIcon = () => (
  <S>
    <circle cx="5.5" cy="8" r="3" stroke="#e5c07b" strokeWidth="1.4" />
    <path d="M8.5 8H14M12 8v2.5" stroke="#e5c07b" strokeWidth="1.4" />
  </S>
);

export const FkIcon = () => (
  <S>
    <circle cx="5.5" cy="8" r="3" stroke="#a7aab2" strokeWidth="1.4" />
    <path d="M8.5 8H14M12 8v2.5" stroke="#a7aab2" strokeWidth="1.4" />
  </S>
);

export const IndexIcon = () => (
  <S>
    <path d="M3 3.5h10M3 6.5h7M3 9.5h10M3 12.5h5" stroke="#b4b8bf" strokeWidth="1.2" strokeLinecap="round" />
  </S>
);

/* ── Tab icons for what has no tree node (same 16px style) ── */
/** Query console: terminal prompt */
export const ConsoleIcon = () => (
  <S>
    <rect x="2" y="3" width="12" height="10" rx="1.2" stroke="var(--ui-icon)" strokeWidth="1.1" />
    <path d="M4.8 6.3 6.8 8l-2 1.7M8.2 10.2h3" stroke="var(--ui-icon-db)" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
  </S>
);

/** SQL text (DDL of an object without its own icon, SQL Generator) */
export const DdlIcon = () => (
  <S>
    <path d="M4 2.5h5.5L12.5 5.5v8H4Z" stroke="var(--ui-icon)" strokeWidth="1.1" strokeLinejoin="round" />
    <path d="M6 7.5h4.5M6 9.7h4.5M6 11.9h3" stroke="var(--ui-icon-db)" strokeWidth="1" strokeLinecap="round" />
  </S>
);

/** DBA report */
export const ReportIcon = () => (
  <S>
    <path d="M2.5 13.5h11" stroke="var(--ui-icon)" strokeWidth="1.1" strokeLinecap="round" />
    <path d="M4.5 11V8M8 11V4.5M11.5 11V6.5" stroke="var(--ui-icon-db)" strokeWidth="1.8" strokeLinecap="round" />
  </S>
);

/** ER diagram: two tables and a relation */
export const DiagramIcon = () => (
  <S>
    <rect x="1.8" y="2.5" width="5.4" height="4.5" rx=".8" stroke="var(--ui-icon)" strokeWidth="1.1" />
    <rect x="8.8" y="9" width="5.4" height="4.5" rx=".8" stroke="var(--ui-icon)" strokeWidth="1.1" />
    <path d="M4.5 7v4.2h4.3" stroke="var(--ui-icon-db)" strokeWidth="1.1" strokeLinejoin="round" />
  </S>
);

/** Schema compare / migration */
export const MigrateIcon = () => (
  <S>
    <path d="M2.5 5.5h10M10 3l2.5 2.5L10 8M13.5 10.5h-10M6 8l-2.5 2.5L6 13" stroke="var(--ui-icon-db)" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
  </S>
);

/** RAG (ask the schema) */
export const RagIcon = () => (
  <S>
    <path d="M7 2.5c.4 2.6 1.4 3.6 4 4-2.6.4-3.6 1.4-4 4-.4-2.6-1.4-3.6-4-4 2.6-.4 3.6-1.4 4-4Z" fill="var(--ui-icon-db)" />
    <path d="M12 9.5c.2 1.2.7 1.7 1.9 1.9-1.2.2-1.7.7-1.9 1.9-.2-1.2-.7-1.7-1.9-1.9 1.2-.2 1.7-.7 1.9-1.9Z" fill="var(--ui-icon)" />
  </S>
);

type Icon = React.FC;
/** Explorer tree object kind → its icon (the tree, Search Everywhere and the tabs that show the object) */
const OBJECT_ICON: Record<string, Icon> = {
  schema: SchemaIcon,
  table: TableIcon,
  partitioned: TableIcon,
  foreign: TableIcon,
  view: ViewIcon,
  matview: ViewIcon,
  sequence: SequenceIcon,
  routine: RoutineIcon,
  function: RoutineIcon,
  procedure: RoutineIcon,
  column: ColumnIcon,
};
export const objectIcon = (kind?: string): Icon | undefined => (kind ? OBJECT_ICON[kind] : undefined);

const TAB_ICON: Record<string, Icon> = { console: ConsoleIcon, table: TableIcon, ddl: DdlIcon, redis: RedisIcon, dba: ReportIcon, erd: DiagramIcon, migrate: MigrateIcon, rag: RagIcon };
/** Tab kind (+ the object kind a table or DDL tab shows) → its icon: the tab bar, Switcher, Recent Files and Search Everywhere */
export const tabIcon = (kind: string, obj?: string): Icon => ((kind === "table" || kind === "ddl") && objectIcon(obj)) || TAB_ICON[kind] || DdlIcon;
/** A tab's icon as drawn: an object's DDL tab adds a small page mark in the corner, so it doesn't look like the object's data tab */
export const TabIcon = ({ kind, obj }: { kind: string; obj?: string }) => {
  const I = tabIcon(kind, obj);
  if (kind !== "ddl" || I === DdlIcon) return <I />;
  return (
    <span className="ui-ddl-mark">
      <I />
      <svg width="8" height="9" viewBox="0 0 8 9" aria-hidden>
        <path d="M.5.5h4.5l2.5 2.5v5.5h-7Z" fill="var(--ui-surface)" stroke="var(--ui-icon-db)" strokeLinejoin="round" />
        <path d="M2 4.5h3.5M2 6.5h2.5" stroke="var(--ui-icon-db)" strokeWidth=".9" strokeLinecap="round" />
      </svg>
    </span>
  );
};

/* ── Grid toolbar icons (16px line icons) ── */
const L = ({ d, children }: { d?: string; children?: React.ReactNode }) => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    {d && <path d={d} />}
    {children}
  </svg>
);
export const IcReload = () => <L d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5V5h-2.5" />;
export const IcClock = () => (
  <L>
    <circle cx="8" cy="8" r="5.5" />
    <path d="M8 5v3.2l2 1.3" />
  </L>
);
export const IcStop = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden>
    <rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" />
  </svg>
);
export const IcPlus = () => <L d="M8 3v10M3 8h10" />;
export const IcMinus = () => <L d="M3 8h10" />;
export const IcRevert = () => <L d="M4 6h6a3 3 0 0 1 0 6H6M6 3.5 3.5 6 6 8.5" />;
export const IcPreview = () => (
  <L>
    <path d="M2 8s2.2-4 6-4 6 4 6 4-2.2 4-6 4-6-4-6-4Z" />
    <circle cx="8" cy="8" r="1.7" />
  </L>
);
export const IcSubmit = () => <L d="M8 12.5V4M4.5 7 8 3.5 11.5 7M3.5 13h9" />;
export const IcSearch = () => (
  <L>
    <circle cx="7" cy="7" r="4" />
    <path d="m10 10 3.5 3.5" />
  </L>
);
export const IcExport = () => <L d="M8 2.5v8M5 7.5l3 3 3-3M3 13h10" />;
export const IcImport = () => <L d="M8 10.5v-8M5 5.5l3-3 3 3M3 13h10" />;
export const IcView = () => (
  <L>
    <path d="M1.8 8s2.3-4.3 6.2-4.3S14.2 8 14.2 8 11.9 12.3 8 12.3 1.8 8 1.8 8Z" />
    <circle cx="8" cy="8" r="2" />
  </L>
);
export const IcGear = () => (
  <L>
    <circle cx="8" cy="8" r="2" />
    <path d="M8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2" />
  </L>
);
export const IcKeyboard = () => (
  <L>
    <rect x="1.8" y="4" width="12.4" height="8" rx="1.3" />
    <path d="M4.3 6.6h.1M6.8 6.6h.1M9.2 6.6h.1M11.7 6.6h.1M4.3 9.4h.1M11.7 9.4h.1M6.5 9.4h3" />
  </L>
);
export const IcFilter = () => <L d="M2.5 3.5h11L9.5 8.5V13l-3-1.5v-3l-4-5Z" />;
export const IcOrder = () => <L d="M3 4h10M3 8h7M3 12h4" />;
/** Collapse all (DataGrip Collapse All) */
export const IcCollapseAll = () => <L d="M4.5 3 8 6.5 11.5 3M4.5 13 8 9.5l3.5 3.5" />;
/** More (three vertical dots) */
export const IcMoreV = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
    <circle cx="8" cy="3.5" r="1.2" />
    <circle cx="8" cy="8" r="1.2" />
    <circle cx="8" cy="12.5" r="1.2" />
  </svg>
);
/** About (ⓘ): opens an explanation (InfoTip) */
export const IcInfo = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" aria-hidden>
    <circle cx="8" cy="8" r="6.2" />
    <path d="M8 7.2v4" />
    <circle cx="8" cy="4.9" r=".45" fill="currentColor" />
  </svg>
);
export const IcChart = () => <L d="M2.5 13.5h11M4 11V8M7 11V4.5M10 11V6.5M13 11V9" />;
export const IcTranspose = () => <L d="M3 3h4v4H3zM9 9h4v4H9zM9 4.5h2.5V7M7 11.5H4.5V9" />;
/** GitHub mark (Settings › Updates: Open GitHub) */
export const IcGitHub = () => (
  <svg width="22" height="22" viewBox="0 0 16 16" aria-hidden>
    <path
      fill="currentColor"
      d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
    />
  </svg>
);

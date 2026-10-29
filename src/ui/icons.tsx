/** 트리 아이콘 (직접 그린 16px SVG — DataGrip과 같은 형태·색감) */
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

/** PostgreSQL 데이터 소스 (코끼리) */
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

export const DatabaseIcon = () => (
  <S>
    <rect x="3" y="2.5" width="10" height="11" rx="2" fill="#3574f0" />
    <rect x="5" y="4.5" width="6" height="7" rx=".8" stroke="#dce6ff" strokeWidth="1.1" />
  </S>
);

export const SchemaIcon = () => (
  <S>
    <rect x="2" y="2" width="5" height="5" rx="1" stroke="#a7aab2" strokeWidth="1.1" />
    <rect x="9" y="9" width="5" height="5" rx="1" stroke="#a7aab2" strokeWidth="1.1" />
    <rect x="2" y="9" width="5" height="5" rx="1" stroke="#a7aab2" strokeWidth="1.1" />
    <path d="M7 4.5h3.5V9" stroke="#a7aab2" strokeWidth="1.1" />
  </S>
);

export const FolderIcon = () => (
  <S>
    <path d="M1.8 4.2c0-.7.5-1.2 1.2-1.2h3.3l1.4 1.5h5.3c.7 0 1.2.5 1.2 1.2v6.1c0 .7-.5 1.2-1.2 1.2H3c-.7 0-1.2-.5-1.2-1.2V4.2Z" fill="#3574f0" />
  </S>
);

export const TableIcon = () => (
  <S>
    <rect x="2" y="2.5" width="12" height="11" rx="1.2" stroke="#b4b8bf" strokeWidth="1.1" />
    <path d="M2 6.2h12M2 9.8h12M6 2.5v11M10 2.5v11" stroke="#b4b8bf" strokeWidth="1" />
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

/* ── 그리드 툴바 아이콘 (16px 선 아이콘) ── */
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
export const IcFilter = () => <L d="M2.5 3.5h11L9.5 8.5V13l-3-1.5v-3l-4-5Z" />;
export const IcOrder = () => <L d="M3 4h10M3 8h7M3 12h4" />;
export const IcChart = () => <L d="M2.5 13.5h11M4 11V8M7 11V4.5M10 11V6.5M13 11V9" />;
export const IcTranspose = () => <L d="M3 3h4v4H3zM9 9h4v4H9zM9 4.5h2.5V7M7 11.5H4.5V9" />;

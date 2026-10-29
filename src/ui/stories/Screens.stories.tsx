import type { Meta, StoryObj } from "@storybook/react-vite";
// 가짜 백엔드 (앱의 ?mock 과 같다): 화면이 부르는 명령에 정해진 데이터를 돌려준다
import "../../lib/mock";
import "../../lib/monaco";
import App from "../../App";
import { dialectOf, type ConnectionView, type QueryResult } from "../../lib/api";
import { ConnectionDialog } from "../../components/ConnectionDialog";
import { CopyRowsDialog, GenerateDataDialog } from "../../components/DataTools";
import { DbaReport } from "../../components/DbaReport";
import { ErdView } from "../../components/ErdView";
import { FullTextSearchDialog } from "../../components/FullTextSearch";
import { HistoryDialog } from "../../components/HistoryDialog";
import { KeymapDialog } from "../../components/KeymapDialog";
import { MigrateView } from "../../components/MigrateView";
import { NewObjectDialog, TableDesigner } from "../../components/ObjectEditors";
import { ProductionLog } from "../../components/ProductionLog";
import { RagView } from "../../components/RagView";
import { RedisBrowser } from "../../components/RedisBrowser";
import { ResultChart } from "../../components/ResultChart";
import { ResultGrid } from "../../components/ResultGrid";
import { SearchEverywhere } from "../../components/SearchEverywhere";
import { Switcher } from "../../components/Switcher";
import { BackupDialog, ExportDialog, ImportDialog } from "../../components/Transfer";
import { UpdateDialog } from "../../components/UpdateDialog";
import { WorkspaceBar } from "../../components/WorkspaceBar";

/** 화면: 실제 화면 컴포넌트를 가짜 데이터로 (앱의 ?mock 과 같은 데이터). 버튼은 눌러 볼 수 있다 */
const meta: Meta = { title: "GUI/5 Screens", parameters: { layout: "fullscreen" } };
export default meta;
type S = StoryObj;

const conn = (id: string, name: string, kind: string, color: string | null, extra: Partial<ConnectionView> = {}) =>
  ({ id, name, kind, host: "db.example.com", port: kind === "mysql" ? 3306 : 5432, user: "app", database: "shop_app", color, hasPassword: true, connected: true, ...extra }) as ConnectionView;
const DEV = conn("dev", "[DEV] shop_app", "postgres", "#5f5b24");
const PROD = conn("prod", "[PROD] shop_app", "postgres", "#6e2a2a", { options: { confirmChanges: true } } as Partial<ConnectionView>);
const STAGE = conn("stage", "[STAGE] shop_app", "postgres", "#28406e");
const REDIS = conn("rd", "[Redis] cache", "redis", "#6e2a2a", { database: "0" });
const CONNS = [DEV, PROD, STAGE, REDIS];
const noop = () => {};

const sales: QueryResult = {
  columns: [
    { name: "region", typeName: "TEXT" },
    { name: "month", typeName: "DATE" },
    { name: "orders", typeName: "INT8" },
    { name: "revenue", typeName: "NUMERIC" },
  ],
  rows: ["Seoul", "Busan", "Incheon", "Daegu"].flatMap((r, i) =>
    ["2026-07-01", "2026-08-01", "2026-09-01", "2026-10-01"].map((m, j) => [r, m, 120 - i * 20 + j * 7, ((4 - i) * 12000 + j * 1500).toFixed(2)]),
  ),
  rowsAffected: 0,
  truncated: false,
  elapsedMs: 12,
  hasResultSet: true,
};

const Frame = ({ children, h = 560 }: { children: React.ReactNode; h?: number }) => <div style={{ height: h, display: "flex", flexDirection: "column", border: "1px solid var(--ui-border)" }}>{children}</div>;

/** 앱 전체 (탐색기 · 탭 · 콘솔). 탐색기 우클릭, ⇧⌘A 로 Find Action, 콘솔에서 ⌘↩ 실행 */
export const FullApp: S = { name: "App (whole window)", render: () => <div style={{ height: "100vh" }}><App /></div> };

export const NewDataSource: S = { name: "Data Source — new", render: () => <ConnectionDialog onClose={noop} onSaved={noop} /> };
export const EditDataSource: S = { name: "Data Source — edit (SSH)", render: () => <ConnectionDialog edit={{ ...PROD, ssh: { enabled: true, host: "bastion.example.com", port: 22, user: "ec2-user", auth: "key", keyPath: "~/.ssh/id_ed25519" } } as ConnectionView} onClose={noop} onSaved={noop} /> };

export const TableNew: S = { name: "New Table", render: () => <TableDesigner target="dev" conn={DEV} schema="public" onDone={noop} onOpenConsole={noop} onClose={noop} /> };
export const TableModify: S = { name: "Modify Table (⌘F6)", render: () => <TableDesigner target="dev" conn={DEV} schema="public" table="customers" onDone={noop} onOpenConsole={noop} onClose={noop} /> };
export const NewView: S = { name: "New View", render: () => <NewObjectDialog kind="view" target="dev" conn={DEV} schema="public" onDone={noop} onOpenConsole={noop} onClose={noop} /> };
export const NewDatabase: S = { name: "New Database", render: () => <NewObjectDialog kind="database" target="dev" conn={DEV} schema="public" onDone={noop} onOpenConsole={noop} onClose={noop} /> };

export const ProdLog: S = { name: "Production Log", render: () => <ProductionLog connId={null} onOpenUndo={noop} onClose={noop} /> };
export const GenerateData: S = { name: "Generate Test Data", render: () => <GenerateDataDialog target="dev" schema="public" table="orders" onDone={noop} onClose={noop} /> };
export const CopyRows: S = { name: "Copy Related Rows", render: () => <CopyRowsDialog source="prod" schema="public" table="users" conn={PROD} conns={CONNS} onDone={noop} onClose={noop} /> };

export const Chart: S = { name: "Result Chart", render: () => <Frame><ResultChart result={sales} /></Frame> };
export const Grid: S = {
  name: "Result Grid",
  render: () => (
    <Frame>
      <ResultGrid result={sales} running={false} loadingMore={false} query="select region, month, orders, revenue from sales" onFetchMore={noop} onSubmit={async () => {}} onReload={noop} onCancel={noop} onExport={noop} />
    </Frame>
  ),
};

export const History: S = { name: "Query History", render: () => <HistoryDialog connId="dev" title="[DEV] shop_app Query History" onPick={noop} onClose={noop} /> };
export const Search: S = {
  name: "Search Everywhere (⇧⇧)",
  render: () => (
    <div style={{ height: 520, position: "relative" }}>
      <SearchEverywhere
        initialTab="all"
        conns={CONNS}
        contextConnId="dev"
        files={[{ key: "f1", icon: "⌘", title: "console [DEV] shop_app", hint: "[DEV] shop_app", open: true }]}
        actions={[{ name: "Production Log…", group: "Run", run: noop }, { name: "Generate Test Data…", group: "Data", run: noop }]}
        onClose={noop}
        onSelectInTree={noop}
        onOpenData={noop}
        onDdl={noop}
        onQuickDoc={noop}
        onOpenFile={noop}
      />
    </div>
  ),
};
export const FindInData: S = { name: "Full-text Search", render: () => <FullTextSearchDialog scope={{ connId: "dev", connName: "[DEV] shop_app", db: "shop_app", schemas: ["public"], tables: [] }} onClose={noop} onSearch={noop} /> };
export const SwitcherStory: S = {
  name: "Switcher (⌃⇥)",
  render: () => (
    <Switcher
      items={[
        { key: "a", icon: "⌘", title: "console [DEV] shop_app", hint: "[DEV] shop_app", open: true, color: "#5f5b24" },
        { key: "b", icon: "▦", title: "users", hint: "[PROD] shop_app · public", open: true, color: "#6e2a2a" },
        { key: "c", icon: "⧉", title: "Sessions & Locks", hint: "[PROD] shop_app", open: true },
      ]}
      start={1}
      onPick={noop}
      onClose={noop}
    />
  ),
};
export const Keymap: S = { name: "Keymap Reference (⇧⌘/)", render: () => <KeymapDialog onClose={noop} onEdit={noop} /> };

export const Export: S = { name: "Export Data", render: () => <ExportDialog what="public.users" onExport={noop} onClose={noop} /> };
export const Import: S = { name: "Import Data", render: () => <ImportDialog target="dev" schema="public" table="users" onImport={noop} onClose={noop} /> };
export const Backup: S = { name: "Back Up Database", render: () => <BackupDialog what="shop_app" pg onRun={noop} onClose={noop} /> };
export const Update: S = {
  name: "Update available",
  render: () => (
    <UpdateDialog
      info={{ current: "0.2.5", latest: { version: "0.2.6", url: "https://github.com/Hkhyyy/qurry/releases/tag/v0.2.6", notes: "", assets: [] }, changes: [{ version: "0.2.6", changes: ["[feat] 데이터 작업 · 결과 차트 · 운영 안전", "[fix] 탐색기 가로 스크롤"] }] } as never}
      onClose={noop}
    />
  ),
};

export const Diagram: S = { name: "Diagram (ERD)", render: () => <Frame h={600}><ErdView target="dev" schemas={["public"]} visible onOpenTable={noop} onStatus={noop} /></Frame> };
export const Dba: S = {
  name: "Database Admin",
  render: () => (
    <Frame h={600}>
      <DbaReport target="prod" connName="[PROD] shop_app" report="sessions" readOnly={false} dialect={dialectOf("postgres")} visible onReport={noop} onOpenConsole={noop} onStatus={noop} />
    </Frame>
  ),
};
export const Migrate: S = { name: "Compare & Migrate", render: () => <Frame h={600}><MigrateView conns={CONNS} initialConnId="dev" onOpenConsole={noop} onStatus={noop} /></Frame> };
export const Rag: S = { name: "AI Search (RAG)", render: () => <Frame h={600}><RagView conns={CONNS} runJob={async (_l, run) => run("job")} onStatus={noop} /></Frame> };
export const Redis: S = {
  name: "Redis Browser",
  render: () => (
    <Frame h={600}>
      <RedisBrowser target="rd" connName="[Redis] cache" database="0" dbList={["db0", "db3"]} readOnly={false} onSwitchDb={noop} onLoadDbs={noop} onStatus={noop} />
    </Frame>
  ),
};
export const Workspaces: S = {
  name: "Workspace rail",
  render: () => (
    <div style={{ width: 64, height: 300, background: "var(--ui-surface-2)" }}>
      <WorkspaceBar
        list={[
          { id: "a", name: "Default", connIds: [] },
          { id: "b", name: "Reports", connIds: [] },
        ]}
        current="a"
        counts={{ a: 3, b: 1 }}
        shortcut={(n) => `⌘${n}`}
        onSwitch={noop}
        onNew={noop}
        onRename={noop}
        onDelete={noop}
      />
    </div>
  ),
};

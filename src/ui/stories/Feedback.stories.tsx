import type { Meta, StoryObj } from "@storybook/react-vite";
import { Button, EmptyState, Notice } from "..";

const meta: Meta = { title: "GUI/Feedback" };
export default meta;

export const Notices: StoryObj = {
  render: () => (
    <div style={{ width: 560, display: "flex", flexDirection: "column", gap: 8 }}>
      <Notice>Runs pg_dump on this computer. The password is passed through the environment.</Notice>
      <Notice tone="ok">✓ PostgreSQL 16.4 on aarch64</Notice>
      <Notice tone="warn">2 consoles have uncommitted changes.</Notice>
      <Notice tone="danger">ERROR: relation "no_such_table" does not exist</Notice>
    </div>
  ),
};

export const Empty: StoryObj = {
  render: () => (
    <div style={{ width: 420, border: "1px solid var(--ui-border)" }}>
      <EmptyState title="No snippets yet" action={<Button size="sm">Save Current Query</Button>}>
        Save a query you use often to find it here.
      </EmptyState>
    </div>
  ),
};

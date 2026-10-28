import type { Meta, StoryObj } from "@storybook/react-vite";
import { Badge, Button, IconButton, Spacer, ToolButton, Toolbar, ToolbarSep } from "..";

const meta: Meta = { title: "GUI/Buttons" };
export default meta;

/** variant · size. 주 버튼(primary)은 창마다 하나, 맨 오른쪽 */
export const Button_: StoryObj = {
  name: "Button",
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <Button>Default</Button>
        <Button variant="primary">Primary</Button>
        <Button variant="danger">Delete</Button>
        <Button variant="ghost">Ghost</Button>
        <Button variant="link">Link</Button>
        <Button disabled>Disabled</Button>
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <Button size="sm">Small</Button>
        <Button size="sm" variant="primary">
          Small primary
        </Button>
        <Button size="sm" variant="danger">
          Small danger
        </Button>
      </div>
    </div>
  ),
};

/** 아이콘 버튼은 title 필수 (마우스를 올리면 이름 · 단축키) */
export const IconButtons: StoryObj = {
  render: () => (
    <div style={{ display: "flex", gap: 4 }}>
      <IconButton title="Execute (⌘↩)" tone="ok">
        ▶
      </IconButton>
      <IconButton title="Cancel (⌘F2)" tone="danger">
        ■
      </IconButton>
      <IconButton title="History (⌥⌘e)">⏱</IconButton>
      <IconButton title="Find (⌘f)" active>
        ⌕
      </IconButton>
      <IconButton title="Disabled" disabled>
        ＋
      </IconButton>
      <IconButton title="Small" size="sm">
        ⟳
      </IconButton>
    </div>
  ),
};

/** 콘솔 도구 막대: 아이콘 · 글자 버튼 · 구분선 · 오른쪽 정렬 */
export const ToolbarStory: StoryObj = {
  name: "Toolbar",
  render: () => (
    <div style={{ border: "1px solid var(--ui-border)", width: 760 }}>
      <Toolbar>
        <IconButton title="Execute (⌘↩)" tone="ok">
          ▶
        </IconButton>
        <ToolButton>Explain ▾</ToolButton>
        <IconButton title="History">⏱</IconButton>
        <ToolbarSep />
        <ToolButton active>Tx: Manual ▾</ToolButton>
        <ToolButton accent>✓ Commit</ToolButton>
        <ToolButton>↶ Roll back</ToolButton>
        <IconButton title="Cancel" tone="danger" disabled>
          ■
        </IconButton>
        <Spacer />
        <span className="ui-muted">[PROD] kr</span>
        <Badge tone="danger">PROD</Badge>
      </Toolbar>
    </div>
  ),
};

export const Badges: StoryObj = {
  render: () => (
    <div style={{ display: "flex", gap: 8 }}>
      <Badge>66</Badge>
      <Badge onClick={() => {}}>1 of 3</Badge>
      <Badge tone="accent">SSH</Badge>
      <Badge tone="warn">Tx</Badge>
      <Badge tone="danger">PROD</Badge>
    </div>
  ),
};

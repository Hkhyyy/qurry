import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Avatar, Badge, Button, Checkbox, Code, CodeBlock, Divider, IconButton, Kbd, ProgressBar, Radio, Row, Select, Spacer, Swatch, Text, TextArea, TextInput, ToolButton } from "..";

/** 원자: 더 쪼갤 수 없는 조각. 화면 코드는 맨 HTML 대신 이것을 쓴다 */
const meta: Meta = { title: "GUI/1 Atoms" };
export default meta;
type S = StoryObj;

const Box = ({ title, children }: { title: string; children: React.ReactNode }) => (
  <div style={{ marginBottom: 20 }}>
    <Text tone="muted" size="xs" as="div" style={{ marginBottom: 8 }}>
      {title}
    </Text>
    {children}
  </div>
);

export const TextStory: S = {
  name: "Text",
  render: () => (
    <>
      <Box title="tone">
        <Row>
          {(["default", "muted", "subtle", "accent", "ok", "warn", "danger"] as const).map((t) => (
            <Text key={t} tone={t}>
              {t}
            </Text>
          ))}
        </Row>
      </Box>
      <Box title="size · strong · mono">
        <Row>
          <Text size="xs">xs 11</Text>
          <Text size="sm">sm 12</Text>
          <Text size="md">md 13</Text>
          <Text size="lg">lg 14</Text>
          <Text strong>strong</Text>
          <Text mono>mono_text</Text>
        </Row>
      </Box>
      <Box title="truncate (마우스를 올리면 전체)">
        <div style={{ width: 220 }}>
          <Text truncate>[PROD] rentalpay · app@db.example.com · rentalpay_main_database</Text>
        </div>
      </Box>
    </>
  ),
};

export const CodeStory: S = {
  name: "Code · CodeBlock · Kbd",
  render: () => (
    <>
      <Box title="Code (줄 안)">
        <Text>
          Run <Code>select * from users</Code> first.
        </Text>
      </Box>
      <Box title="CodeBlock (여러 줄)">
        <CodeBlock style={{ width: 520 }}>{"ALTER TABLE public.orders ADD COLUMN user_id integer;\nALTER TABLE public.users ADD COLUMN email text;"}</CodeBlock>
      </Box>
      <Box title="Kbd (단축키)">
        <Row>
          <Kbd>⌘k</Kbd>
          <Kbd>⇧⌘A</Kbd>
          <Kbd>⌥⌘e</Kbd>
          <Kbd>F5</Kbd>
        </Row>
      </Box>
    </>
  ),
};

export const Layout: S = {
  name: "Row · Spacer · Divider",
  render: () => (
    <div style={{ width: 520, border: "1px dashed var(--ui-border)", padding: 8 }}>
      <Row gap={2}>
        <Text>left</Text>
        <Divider vertical />
        <Text>after divider</Text>
        <Spacer />
        <Text tone="muted">right (Spacer)</Text>
      </Row>
      <Divider />
      <Text tone="muted" size="sm">
        horizontal Divider 위
      </Text>
    </div>
  ),
};

export const Buttons: S = {
  render: () => (
    <>
      <Box title="Button · variant">
        <Row gap={3}>
          <Button>Default</Button>
          <Button variant="primary">Primary</Button>
          <Button variant="danger">Delete</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="link">Link</Button>
          <Button disabled>Disabled</Button>
        </Row>
      </Box>
      <Box title="Button · size sm">
        <Row gap={3}>
          <Button size="sm">Small</Button>
          <Button size="sm" variant="primary">
            Small primary
          </Button>
        </Row>
      </Box>
      <Box title="IconButton (title 필수) · tone · active">
        <Row gap={1}>
          <IconButton title="Execute (⌘↩)" tone="ok">
            ▶
          </IconButton>
          <IconButton title="Cancel" tone="danger">
            ■
          </IconButton>
          <IconButton title="History">⏱</IconButton>
          <IconButton title="Find" active>
            ⌕
          </IconButton>
          <IconButton title="Disabled" disabled>
            ＋
          </IconButton>
          <IconButton title="Small" size="sm">
            ⟳
          </IconButton>
        </Row>
      </Box>
      <Box title="ToolButton (도구 막대 글자 버튼) · active · accent">
        <Row gap={1}>
          <ToolButton>Explain ▾</ToolButton>
          <ToolButton active>Tx: Manual ▾</ToolButton>
          <ToolButton accent>✓ Commit</ToolButton>
          <ToolButton disabled>↶ Roll back</ToolButton>
        </Row>
      </Box>
    </>
  ),
};

export const Badges: S = {
  name: "Badge · Avatar · Swatch · ProgressBar",
  render: () => (
    <>
      <Box title="Badge · tone · color (Redis 타입)">
        <Row gap={2}>
          <Badge>66</Badge>
          <Badge onClick={() => {}}>1 of 3</Badge>
          <Badge tone="accent">SSH</Badge>
          <Badge tone="warn">⚡</Badge>
          <Badge tone="danger">PROD</Badge>
          <Badge color="#5f9ea0">STRING</Badge>
          <Badge color="#b07cc6">HASH</Badge>
        </Row>
      </Box>
      <Box title="Avatar (워크스페이스 이니셜)">
        <Row gap={2}>
          <Avatar active>DE</Avatar>
          <Avatar>RE</Avatar>
        </Row>
      </Box>
      <Box title="Swatch (Data Source 색)">
        <Row gap={2}>
          <Swatch color={null} />
          <Swatch color="#6e2a2a" selected />
          <Swatch color="#2f5634" />
          <Swatch color="#28406e" />
        </Row>
      </Box>
      <Box title="ProgressBar · 알 수 있음 / 모름">
        <Row gap={4}>
          <ProgressBar pct={62} />
          <ProgressBar pct={null} />
        </Row>
      </Box>
    </>
  ),
};

export const Inputs: S = {
  render: function Render() {
    const [c, setC] = useState(true);
    const [r, setR] = useState("a");
    return (
      <div style={{ width: 420 }}>
        <Box title="TextInput · full · grow · disabled">
          <TextInput full placeholder="Name" />
          <Row gap={3} style={{ marginTop: 8 }}>
            <TextInput grow defaultValue="db.example.com" />
            <TextInput className="w-port" defaultValue="5432" />
          </Row>
          <TextInput full disabled defaultValue="disabled" style={{ marginTop: 8 }} />
        </Box>
        <Box title="Select">
          <Select full defaultValue="postgres">
            <option value="postgres">PostgreSQL</option>
            <option value="mysql">MySQL</option>
          </Select>
        </Box>
        <Box title="TextArea (고정폭)">
          <TextArea style={{ width: "100%" }} rows={3} placeholder="SET search_path TO app, public;" />
        </Box>
        <Box title="Checkbox · indeterminate · Radio">
          <Row gap={4}>
            <Checkbox checked={c} onChange={setC}>
              Read-only
            </Checkbox>
            <Checkbox checked={false} indeterminate onChange={() => {}}>
              Some schemas
            </Checkbox>
            <Radio checked={r === "a"} onChange={() => setR("a")}>
              A
            </Radio>
            <Radio checked={r === "b"} onChange={() => setR("b")}>
              B
            </Radio>
          </Row>
        </Box>
      </div>
    );
  },
};

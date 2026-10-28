import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Checkbox, Field, FormGrid, FormSection, Hint, RadioGroup, Select, Tabs, TextInput } from "..";

const meta: Meta = { title: "GUI/Form" };
export default meta;

/** 한 줄 = 라벨 + 칸. 힌트 · 오류는 칸 아래 */
export const Fields: StoryObj = {
  render: function Render() {
    const [ro, setRo] = useState(true);
    const [scope, setScope] = useState("one");
    return (
      <div style={{ width: 480 }}>
        <Field label="Name">
          <TextInput full defaultValue="[PROD] kr" />
        </Field>
        <Field label="Driver" hint="Drivers are installed when you pick them.">
          <Select full defaultValue="postgres">
            <option value="postgres">PostgreSQL</option>
            <option value="mysql">MySQL</option>
          </Select>
        </Field>
        <Field label="Port" error="Must be a number">
          <TextInput className="w-port" defaultValue="54x2" />
        </Field>
        <Field label="Options">
          <Checkbox checked={ro} onChange={setRo}>
            Read-only
          </Checkbox>
        </Field>
        <Field label="Snippet">
          <RadioGroup value={scope} onChange={setScope} options={[{ value: "one", label: 'Only for "[PROD] kr"' }, { value: "all", label: "For all data sources" }]} />
        </Field>
      </div>
    );
  },
};

/** 긴 설정 창 (Data Source Properties): 구역 제목 + 라벨 · 칸 */
export const Grid: StoryObj = {
  render: function Render() {
    const [ssh, setSsh] = useState(true);
    return (
      <div style={{ width: 620 }}>
        <FormGrid>
          <FormSection>SSH tunnel</FormSection>
          <span />
          <Checkbox checked={ssh} onChange={setSsh}>
            Use SSH tunnel
          </Checkbox>
          <label>Host</label>
          <div className="ui-row">
            <TextInput className="grow" defaultValue="bastion.example.com" disabled={!ssh} />
            <span className="ui-muted">Port</span>
            <TextInput className="w-port" defaultValue="22" disabled={!ssh} />
          </div>
          <label>Auth type</label>
          <Select defaultValue="key" disabled={!ssh}>
            <option value="key">Key pair</option>
            <option value="password">Password</option>
          </Select>
        </FormGrid>
        <Hint>Host keys are checked against ~/.ssh/known_hosts.</Hint>
      </div>
    );
  },
};

export const TabsStory: StoryObj = {
  name: "Tabs",
  render: function Render() {
    const [t, setT] = useState<"general" | "ssh" | "schemas" | "options">("general");
    return (
      <div style={{ width: 520 }}>
        <Tabs
          items={[
            { id: "general", label: "General" },
            { id: "ssh", label: "SSH/SSL ●" },
            { id: "schemas", label: "Schemas" },
            { id: "options", label: "Options" },
          ]}
          value={t}
          onChange={setT}
        />
        <div className="ui-muted">{t} tab</div>
      </div>
    );
  },
};

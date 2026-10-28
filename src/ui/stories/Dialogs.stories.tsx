import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect } from "react";
import { Button, ConfirmHost, confirm, ConfirmOptions, Field, Modal, Notice, RadioGroup } from "..";

const meta: Meta = { title: "GUI/Dialogs", parameters: { layout: "fullscreen" } };
export default meta;

/** 대화상자 틀: 제목 · 본문 · 오른쪽 버튼 (취소 → 주 버튼). Esc · 바깥 클릭으로 닫힘 */
export const ModalStory: StoryObj = {
  name: "Modal",
  render: () => (
    <Modal
      title="Back Up kr"
      onClose={() => {}}
      footer={
        <>
          <Button>Cancel</Button>
          <Button variant="primary">Choose File & Back Up</Button>
        </>
      }
    >
      <Field label="Format">
        <RadioGroup value="custom" onChange={() => {}} options={[{ value: "custom", label: "Custom (.dump)", hint: "compressed, restore with Restore…" }, { value: "sql", label: "SQL (.sql)", hint: "plain text" }]} />
      </Field>
      <Notice>Runs pg_dump on this computer (install the client tools if it's missing).</Notice>
    </Modal>
  ),
};

/** 확인 창은 열린 채로 보인다 (confirm() 를 바로 부른다) */
function OpenConfirm(o: ConfirmOptions) {
  useEffect(() => {
    confirm(o);
  }, []);
  return <ConfirmHost />;
}

export const ConfirmDelete: StoryObj = {
  render: () => <OpenConfirm message={'Delete snippet "user orders"?'} okLabel="Delete" danger />,
};

/** 운영 Data Source: 바뀔 행 수를 표로, 많을수록 빨갛게 */
export const ConfirmProduction: StoryObj = {
  render: () => (
    <OpenConfirm
      title="Production: [PROD] kr"
      message="These statements change data. Run them?"
      detail={[
        { text: "update users set name = 'x' where id <= 3", value: "3 rows", tone: "warn" },
        { text: "delete from users", value: "41 rows", tone: "warn" },
        { text: "delete from orders where created_at < now() - interval '1 year'", value: "12,408 rows", tone: "danger" },
        { text: "create table refunds(id int)", value: "schema change", tone: "danger" },
      ]}
      okLabel="Run"
      danger
    />
  ),
};

export const ConfirmTerminate: StoryObj = {
  render: () => <OpenConfirm title="[PROD] kr" message="Terminate Session 1385?" detail={[{ text: "select pg_sleep(600) /* sleeper */", value: "pid 1385" }]} okLabel="Terminate Session" danger />,
};

export const ConfirmChoice: StoryObj = {
  render: () => <OpenConfirm title="Switch to Auto" message="This console has uncommitted changes." okLabel="Commit and Switch" cancelLabel="Stay in Manual" />,
};

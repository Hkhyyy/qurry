import type { StorybookConfig } from "@storybook/react-vite";

// UI 규격 미리 보기 (src/ui/README.md). pnpm storybook
const config: StorybookConfig = {
  framework: "@storybook/react-vite",
  stories: ["../src/**/*.stories.tsx"],
  core: { disableTelemetry: true },
};
export default config;

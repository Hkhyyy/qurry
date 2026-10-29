import type { StorybookConfig } from "@storybook/react-vite";

// UI spec preview (src/ui/README.md). pnpm storybook
const config: StorybookConfig = {
  framework: "@storybook/react-vite",
  stories: ["../src/**/*.stories.tsx"],
  core: { disableTelemetry: true },
};
export default config;

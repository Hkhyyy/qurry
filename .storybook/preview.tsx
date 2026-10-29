import type { Preview } from "@storybook/react-vite";
import { useEffect } from "react";
import "../src/App.css";
import "../src/ui/tokens.css";
import "../src/ui/ui.css";

// Switch the theme from the top toolbar (same data-theme as the app's Settings › Appearance)
const preview: Preview = {
  globalTypes: {
    theme: {
      description: "Theme",
      toolbar: { title: "Theme", icon: "paintbrush", items: ["dark", "light", "hc"], dynamicTitle: true },
    },
  },
  initialGlobals: { theme: "dark" },
  parameters: { layout: "padded", backgrounds: { disable: true } },
  decorators: [
    (Story, ctx) => {
      const theme = ctx.globals.theme as string;
      useEffect(() => {
        document.documentElement.dataset.theme = theme;
      }, [theme]);
      return (
        <div style={{ minHeight: "100vh", padding: 16, margin: -16, background: "var(--ui-bg)", color: "var(--ui-text)", fontFamily: "-apple-system, BlinkMacSystemFont, Inter, sans-serif", fontSize: 13 }}>
          <Story />
        </div>
      );
    },
  ],
};
export default preview;

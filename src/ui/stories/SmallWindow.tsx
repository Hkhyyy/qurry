import type { Decorator } from "@storybook/react-vite";

/** Window sizes the small-window stories offer (the `window` control): the smallest supported and a small laptop one */
const SIZES = ["800x500", "1024x600"];

/**
 * Draws the story in a box the size of a small app window. `transform` makes the box the containing block of `position: fixed`
 * descendants, so a dialog's backdrop (the window) lays out against it, as it would in a window that size; `overflow: hidden` cuts
 * whatever would fall outside, as the window's edge does. No viewport addon is needed
 */
export const smallWindow: Decorator = (Story, ctx) => {
  const [w, h] = String(ctx.args.window ?? SIZES[0]).split("x").map(Number);
  return (
    <div style={{ width: w, height: h, transform: "translate(0)", overflow: "hidden", outline: "1px solid var(--ui-border)" }}>
      <Story />
    </div>
  );
};

/** A story's settings to show it in a small window (800×500 first; 1024×600 in Controls, or `&args=window:1024x600` in the URL) */
export const inSmallWindow = {
  decorators: [smallWindow],
  args: { window: SIZES[0] },
  argTypes: { window: { control: "inline-radio", options: SIZES } },
  parameters: { layout: "fullscreen" },
};

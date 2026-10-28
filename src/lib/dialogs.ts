/**
 * 확인·글 입력 창. 실제 앱(Tauri)에서는 dialog 플러그인이 window.confirm 을 async 함수로 바꿔 넣는다 (Promise 를 돌려준다):
 * `if (!confirm(…)) return` 은 Promise 가 늘 참이라 대답을 기다리지 않고 지나간다. 그래서 늘 `await ask(…)`.
 * window.prompt 는 Tauri 웹뷰에 없다 (null) → 앱 안의 입력 창 (TextDialog, App 이 등록)
 */
export async function ask(message: string): Promise<boolean> {
  const r = window.confirm(message) as unknown as boolean | Promise<boolean>;
  return (await r) === true;
}

export interface TextRequest {
  title: string;
  value: string;
  /** 입력칸 아래에 고를 것 (라디오). choice 는 처음 고른 것 */
  choices?: string[];
  choice?: number;
  resolve: (v: string | null, choice?: number) => void;
}
let show: ((r: TextRequest) => void) | null = null;

/** App 이 입력 창을 띄우는 함수를 등록한다 */
export function registerTextDialog(fn: ((r: TextRequest) => void) | null) {
  show = fn;
}

/** 한 줄 입력. 취소하면 null */
export function askText(title: string, value = ""): Promise<string | null> {
  return new Promise((resolve) => (show ? show({ title, value, resolve }) : resolve(window.prompt(title, value))));
}

/** 한 줄 입력 + 하나 고르기 (예: 스니펫 이름과 보일 범위). 취소하면 null */
export function askTextChoice(title: string, value: string, choices: string[], initial = 0): Promise<{ text: string; choice: number } | null> {
  return new Promise((resolve) => {
    const done = (v: string | null, choice?: number) => resolve(v === null ? null : { text: v, choice: choice ?? initial });
    if (show) show({ title, value, choices, choice: initial, resolve: done });
    else done(window.prompt(title, value));
  });
}

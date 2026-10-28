//! TUI 규격 (GUI 의 src/ui 와 같은 이름 · 같은 색). 화면 코드는 숫자 색 · 손으로 만든 줄 대신 이것을 쓴다.
//! atoms (색 · 글자 조각) → molecules (한 줄: Row · 키 안내 · 폼 칸) → organisms (패널 · 팝업 · 확인 창).
//! 미리 보기: Storybook 의 "TUI" (pnpm tui-snapshots 로 다시 그림)
pub mod atoms;
pub mod molecules;
pub mod organisms;
pub use atoms::*;
pub use molecules::*;
pub use organisms::*;

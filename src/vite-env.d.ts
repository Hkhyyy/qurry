/// <reference types="vite/client" />
declare module "monaco-editor/languages/definitions/pgsql/register";
declare module "monaco-editor/languages/definitions/mysql/register";
declare module "monaco-editor/languages/definitions/mysql/mysql" {
  export const language: { tokenizer: Record<string, unknown[]> } & Record<string, unknown>;
}
declare module "monaco-editor/editor/editor.main";

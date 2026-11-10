/// <reference types="vite/client" />
declare module "monaco-editor/languages/definitions/pgsql/register";
declare module "monaco-editor/languages/definitions/mysql/register";
declare module "monaco-editor/languages/definitions/mysql/mysql" {
  export const language: { tokenizer: Record<string, unknown[]>; keywords: string[] } & Record<string, unknown>;
}
declare module "monaco-editor/languages/definitions/pgsql/pgsql" {
  export const language: { tokenizer: Record<string, unknown[]>; keywords: string[] } & Record<string, unknown>;
}
declare module "monaco-editor/languages/definitions/sql/sql" {
  export const language: { tokenizer: Record<string, unknown[]>; keywords: string[] } & Record<string, unknown>;
}
declare module "monaco-editor/editor/editor.main";

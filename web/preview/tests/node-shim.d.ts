// What the tests use of Node, said once, so tsc can check them without @types/node (the page has no dependency but typescript):
// every `node:` module is untyped, and the few globals and `import.meta` fields the tests read are declared.
declare module 'node:*';
declare const Buffer: { byteLength(s: string): number; from(s: string): Uint8Array };
declare const process: { env: Record<string, string | undefined>; argv: string[]; cwd(): string };
interface ImportMeta { dirname: string; filename: string }

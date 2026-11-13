// The resolve hook behind tests/register.mjs: the import map of index.html, read from the page itself, so there is
// one list of what a bare name means.
import { readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const { imports } = JSON.parse(html.match(/<script type="importmap">([\s\S]*?)<\/script>/)[1]);

export function resolve(specifier, context, next) {
  const to = imports[specifier];
  if (to) return next(pathToFileURL(root.replace(/\/$/, '') + to).href, context);
  return next(specifier, context);
}

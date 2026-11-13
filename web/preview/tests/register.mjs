// Node resolves the page's bare imports the way the page's import map does (index.html): to the vendored files,
// so a test imports exactly the code the page runs.  Loaded with `node --import ./tests/register.mjs`.
import { register } from 'node:module';

register('./vendor-resolve.mjs', import.meta.url);

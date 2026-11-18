/* load.js — Node only: loads the model modules (no DOM) into one Stone, in dependency order, for the benchmarks and the checks. */
const fs = require('fs'), vm = require('vm'), path = require('path');
const MODULES = ['vec', 'rand', 'controls', 'convex', 'region', 'cell', 'layout', 'treat', 'settle', 'marks', 'field', 'mesh', 'stone', 'checks'];
exports.MODULES = MODULES;
exports.source = () => MODULES.map((m) => fs.readFileSync(path.join(__dirname, m + '.js'), 'utf8')).join('\n;\n');
exports.load = () => { const ctx = { console, Date, Math }; vm.createContext(ctx); vm.runInContext(exports.source() + '\n;this.Stone = Stone;', ctx); return ctx.Stone; };

import test from 'node:test';
import assert from 'node:assert/strict';
import { versionParts } from '../js/copy.js';

test('a version splits into the semantic version and the whole build part, dirty included', () => {
  assert.deepEqual(versionParts('0.9.0'), { semver: '0.9.0', build: '', dirty: false });
  assert.deepEqual(versionParts('0.9.0+123.gfd383a7'), { semver: '0.9.0', build: '+123.gfd383a7', dirty: false });
  assert.deepEqual(versionParts('0.9.0+123.gfd383a7.dirty'), { semver: '0.9.0', build: '+123.gfd383a7.dirty', dirty: true });
  assert.deepEqual(versionParts('1.0.0+dirty'), { semver: '1.0.0', build: '+dirty', dirty: true });
  assert.equal(versionParts('1.0.0+dirtywork.3.gabc1234').dirty, false);
});

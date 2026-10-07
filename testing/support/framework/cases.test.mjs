import test from 'node:test';
import assert from 'node:assert/strict';
test('passed_case', () => assert.equal(1, 1));
test('skipped_case', { skip: 'Intentional selector negative control' }, () => {});
test('failed_case', () => assert.fail('Intentional selector negative control'));

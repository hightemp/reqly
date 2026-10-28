import { describe, expect, it } from 'vitest';
import { describeAssertion, needsProperty, needsValue, newAssertion } from './assertions';

describe('assertions', () => {
  it('describes a check in words', () => {
    expect(
      describeAssertion({ source: 'status', property: '', operator: 'equals', expected: '200' }),
    ).toBe('Status equals 200');
    expect(
      describeAssertion({
        source: 'header',
        property: 'Content-Type',
        operator: 'contains',
        expected: 'json',
      }),
    ).toBe('Header Content-Type contains json');
    expect(
      describeAssertion({
        source: 'jsonPath',
        property: '$.id',
        operator: 'exists',
        expected: 'x',
      }),
    ).toBe('JSONPath $.id exists');
    expect(
      describeAssertion({
        source: 'body',
        property: '',
        operator: 'matchesSchema',
        expected: '{}',
      }),
    ).toBe('Body matches JSON Schema');
  });

  it('knows which fields a row needs', () => {
    expect(needsProperty('header')).toBe(true);
    expect(needsProperty('jq')).toBe(true);
    expect(needsProperty('status')).toBe(false);
    expect(needsValue('exists')).toBe(false);
    expect(needsValue('equals')).toBe(true);
  });

  it('starts with a status check', () => {
    const a = newAssertion();
    expect(a).toMatchObject({
      enabled: true,
      source: 'status',
      operator: 'equals',
      value: '200',
    });
    expect(a.id).not.toBe(newAssertion().id);
  });
});

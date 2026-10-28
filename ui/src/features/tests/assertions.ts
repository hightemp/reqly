import type { Assertion } from '$lib/api/generated/Assertion';
import type { AssertionOperator } from '$lib/api/generated/AssertionOperator';
import type { AssertionSource } from '$lib/api/generated/AssertionSource';
import { t, type Key } from '$lib/i18n';

export const SOURCES: AssertionSource[] = [
  'status',
  'responseTime',
  'header',
  'body',
  'jsonPath',
  'jq',
];

export const OPERATORS: AssertionOperator[] = [
  'equals',
  'notEquals',
  'contains',
  'notContains',
  'matches',
  'lessThan',
  'lessThanOrEqual',
  'greaterThan',
  'greaterThanOrEqual',
  'exists',
  'notExists',
  'matchesSchema',
];

/** Header name or expression. */
export const needsProperty = (source: AssertionSource) =>
  source === 'header' || source === 'jsonPath' || source === 'jq';

export const needsValue = (operator: AssertionOperator) =>
  operator !== 'exists' && operator !== 'notExists';

// i18n-dynamic: assertions.sources.
export const sourceLabel = (source: AssertionSource) => t(`assertions.sources.${source}` as Key);

// i18n-dynamic: assertions.operators.
export const operatorLabel = (operator: AssertionOperator) =>
  t(`assertions.operators.${operator}` as Key);

/** "Header Content-Type contains json"; the source label loses its unit. */
export function describeAssertion(check: {
  source: AssertionSource;
  property: string;
  operator: AssertionOperator;
  expected: string;
}): string {
  const parts = [sourceLabel(check.source).replace(/\s*\(.*\)$/, '')];
  if (needsProperty(check.source) && check.property.trim()) parts.push(check.property.trim());
  parts.push(operatorLabel(check.operator));
  if (needsValue(check.operator) && check.operator !== 'matchesSchema') parts.push(check.expected);
  return parts.join(' ');
}

export function newAssertion(): Assertion {
  return {
    id: crypto.randomUUID(),
    enabled: true,
    source: 'status',
    property: '',
    operator: 'equals',
    value: '200',
  };
}

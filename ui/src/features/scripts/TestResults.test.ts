import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { AssertionResult } from '$lib/api/generated/AssertionResult';
import TestResults from './TestResults.svelte';

const result = (patch: Partial<AssertionResult>): AssertionResult => ({
  id: 'a',
  source: 'status',
  property: '',
  operator: 'equals',
  expected: '200',
  passed: true,
  ...patch,
});

describe('TestResults', () => {
  it('lists assertions before script tests with one summary', () => {
    render(TestResults, {
      tests: [{ name: 'script test', passed: false, skipped: false, error: 'boom' }],
      assertions: [
        result({ id: 'a1' }),
        result({ id: 'a2', expected: '201', passed: false, actual: '200' }),
      ],
    });
    const names = screen
      .getAllByRole('listitem')
      .map((li) => li.querySelector('.name')?.textContent);
    expect(names).toEqual(['Status equals 200', 'Status equals 201', 'script test']);
    expect(screen.getByText('1 passed')).toBeInTheDocument();
    expect(screen.getByText('2 failed')).toBeInTheDocument();
    expect(screen.getByText('Got: 200')).toBeInTheDocument();
  });

  it('shows missing values, errors and schema details', () => {
    render(TestResults, {
      tests: [],
      assertions: [
        result({ id: 'a1', source: 'header', property: 'X-Id', operator: 'exists', passed: false }),
        result({
          id: 'a2',
          source: 'jsonPath',
          property: '$..[',
          passed: false,
          error: { code: 'filter.syntax', message: 'bad', params: { detail: 'unexpected [' } },
        }),
        result({
          id: 'a3',
          source: 'body',
          operator: 'matchesSchema',
          passed: false,
          actual: '{}',
          detail: '"email" is a required property',
        }),
      ],
    });
    expect(screen.getByText('No value')).toBeInTheDocument();
    expect(screen.getByText('Invalid expression: unexpected [')).toBeInTheDocument();
    expect(screen.getByText('"email" is a required property')).toBeInTheDocument();
  });
});

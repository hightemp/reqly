import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Environment } from '$lib/api/generated/Environment';
import type { RunItem } from '$lib/api/generated/RunItem';
import type { RunnerInput } from '$lib/api/generated/RunnerInput';
import { RunnerStore } from '$lib/stores/runner.svelte';
import { FakeTransport } from '$lib/testing/fakeTransport';
import RunnerDialog from './RunnerDialog.svelte';

const requests = [
  { id: 'a', name: 'Login', method: 'POST', path: [] },
  { id: 'b', name: 'Profile', method: 'GET', path: ['Users'] },
  { id: 'c', name: 'Logout', method: 'POST', path: [] },
];

const env = (id: string, name: string) => ({ id, name, kind: 'sub' }) as Environment;

const item = (id: string, name: string, passed: boolean): RunItem => ({
  iteration: 0,
  requestId: id,
  name,
  path: [],
  method: 'GET',
  url: `http://x/${id}`,
  status: passed ? 200 : 500,
  totalMs: 12,
  tests: [],
  assertions: [
    {
      id: 'x',
      source: 'status',
      property: '',
      operator: 'equals',
      expected: '200',
      passed,
      actual: passed ? '200' : '500',
    },
  ],
});

function setup() {
  const transport = new FakeTransport();
  const inputs: RunnerInput[] = [];
  transport.handle('runner.run', (input: RunnerInput) => {
    inputs.push(input);
    const items = (input.requestIds ?? []).map((id) =>
      item(id, requests.find((r) => r.id === id)!.name, id !== 'c'),
    );
    return {
      runId: input.runId,
      name: 'C',
      startedAt: 0,
      finishedAt: 1500,
      iterations: 1,
      items,
      cancelled: false,
      stopped: false,
    };
  });
  transport.handle('runner.report', ({ format }) => ({
    fileName: `Demo-report.${format === 'junit' ? 'xml' : format}`,
    text: `<${format}>`,
  }));
  const runner = new RunnerStore(transport);
  runner.open({ collectionId: 'c1', folderId: null, name: 'Demo' });
  const onsave = vi.fn(async () => true);
  render(RunnerDialog, {
    runner,
    requests,
    environments: [env('e1', 'dev'), env('e2', 'prod')],
    activeEnvironment: env('e1', 'dev'),
    onsave,
  });
  return { inputs, runner, onsave };
}

describe('RunnerDialog', () => {
  it('runs the selected requests with the chosen options', async () => {
    const { inputs } = setup();
    expect(screen.getByRole('dialog', { name: 'Run Demo' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Profile' }));
    // Logout moves to the top.
    await fireEvent.click(screen.getAllByRole('button', { name: 'Move up' })[2]!);
    await fireEvent.click(screen.getAllByRole('button', { name: 'Move up' })[1]!);
    await fireEvent.input(screen.getByRole('spinbutton', { name: 'Delay between requests (ms)' }), {
      target: { value: '250' },
    });
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Stop at the first failure' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(inputs).toHaveLength(1));
    expect(inputs[0]).toMatchObject({
      collectionId: 'c1',
      requestIds: ['c', 'a'],
      delayMs: 250,
      stopOnFailure: true,
    });
    expect(inputs[0]!.environmentId).toBeUndefined();
  });

  it('saves a report in the chosen format', async () => {
    const { onsave } = setup();
    await fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    await screen.findByText('2 requests passed');
    await fireEvent.click(screen.getByRole('combobox', { name: 'Report format' }));
    await fireEvent.click(screen.getByRole('option', { name: 'JUnit XML' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Save report' }));
    await waitFor(() =>
      expect(onsave).toHaveBeenCalledWith({
        name: 'Demo-report.xml',
        mime: 'application/xml',
        content: '<junit>',
      }),
    );
  });

  it('shows results and reruns the failed requests', async () => {
    const { inputs } = setup();
    await fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    expect(await screen.findByText('2 requests passed')).toBeInTheDocument();
    expect(screen.getByText('1 request failed')).toBeInTheDocument();
    expect(screen.getByText('Took 1.5 s')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /Logout/ }));
    expect(screen.getByText('Status equals 200')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Rerun failed' }));
    await waitFor(() => expect(inputs).toHaveLength(2));
    expect(inputs[1]!.requestIds).toEqual(['c']);
    await fireEvent.click(screen.getByRole('button', { name: 'New run' }));
    expect(screen.getByRole('checkbox', { name: 'Login' })).toBeInTheDocument();
  });
});

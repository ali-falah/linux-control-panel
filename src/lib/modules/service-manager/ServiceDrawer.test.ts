// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import ServiceDrawer from './ServiceDrawer.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../components/CodeEditor.svelte', async () => {
  const stub = await import('../../components/__test__/CodeEditorStub.svelte');
  return { default: stub.default };
});
const invokeMock = vi.mocked(invoke);

const unit = {
  name: 'nginx.service',
  load_state: 'loaded',
  active_state: 'active',
  sub_state: 'running',
  description: 'Nginx',
  unit_file_state: 'enabled',
};

const props = {
  open: true,
  unit,
  panel: 'logs' as const,
  scope: false,
  recoveryWarning: null,
  onwarning: vi.fn(),
  onretry: vi.fn(),
};

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
});

describe('ServiceDrawer loading', () => {
  it('loads logs when the drawer opens for the logs panel', async () => {
    invokeMock.mockResolvedValue('journal line');

    const { findByText } = render(ServiceDrawer, { props });

    expect(await findByText('journal line')).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith('get_service_logs', {
      name: 'nginx.service',
      lines: 100,
      userMode: false,
    });
  });

  it('loads dependencies when the panel changes', async () => {
    invokeMock.mockResolvedValue({
      requires: ['network.target'],
      wants: [],
      after: [],
      before: [],
    });

    const { findByText } = render(ServiceDrawer, {
      props: { ...props, panel: 'dependencies' },
    });

    expect(await findByText('network.target')).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith('get_unit_dependencies', {
      name: 'nginx.service',
      userMode: false,
    });
  });

  it('opens the diff review modal from Save Override', async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === 'read_unit_override') return '[Service]\nExecStart=/bin/old\n';
      return '';
    });

    const { findByRole, findByText, getByRole } = render(ServiceDrawer, {
      props: { ...props, panel: 'editor' },
    });

    const editor = await findByRole('textbox');
    await fireEvent.input(editor, {
      target: { value: '[Service]\nExecStart=/bin/new\n' },
    });

    const save = getByRole('button', { name: /save override/i });
    expect((save as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(save);

    expect(await findByText('Review nginx.service Override Changes')).toBeTruthy();
  });

  it('does not load anything while closed', async () => {
    render(ServiceDrawer, { props: { ...props, open: false } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

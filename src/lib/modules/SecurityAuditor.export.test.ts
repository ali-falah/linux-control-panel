// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import { uiStore } from '../stores/ui.svelte.ts';
import SecurityAuditor from '../modules/SecurityAuditor.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn() }));
vi.mock('@tauri-apps/api/path', () => ({ downloadDir: vi.fn(async () => '/home/tester/Downloads') }));

const invokeMock = vi.mocked(invoke);

const report = {
  score: 82,
  findings: [
    {
      id: 'ssh_root',
      title: 'SSH root login',
      description: 'Root login is permitted.',
      severity: 'Warning' as const,
      countermeasure: 'Set PermitRootLogin no',
      category: 'SSH Hardening',
      has_auto_fix: true,
      is_resolved: false,
    },
  ],
  category_scores: [{ category: 'SSH Hardening', score: 8, max_score: 10, issues: 1 }],
};

beforeEach(() => {
  uiStore.toasts = [];
  sessionStorage.clear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === 'security_run_audit') return report;
    return null;
  });
});

afterEach(cleanup);

/** The whole point: Export -> menu item -> backend write. */
describe('Security Advisor export', () => {
  it('writes the JSON export through the backend when the menu item is clicked', async () => {
    render(SecurityAuditor);
    await waitFor(() => expect(screen.getByText('Export')).toBeTruthy());

    await fireEvent.click(screen.getByText('Export'));
    const item = await screen.findByRole('menuitem', { name: /Export JSON Data/ });
    await fireEvent.click(item);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        'export_journal_logs_to_file',
        expect.objectContaining({ filename: expect.stringContaining('security-audit') }),
      ),
    );

    const toast = uiStore.toasts.at(-1);
    expect(toast?.type).toBe('success');
  });

  it('writes the HTML export through the backend when the menu item is clicked', async () => {
    render(SecurityAuditor);
    await waitFor(() => expect(screen.getByText('Export')).toBeTruthy());

    await fireEvent.click(screen.getByText('Export'));
    const item = await screen.findByRole('menuitem', { name: /Export HTML Report/ });
    await fireEvent.click(item);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        'export_journal_logs_to_file',
        expect.objectContaining({ filename: expect.stringContaining('.html') }),
      ),
    );
  });

  it('tells the user when the backend refuses', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'security_run_audit') return report;
      throw new Error('permission denied');
    });
    render(SecurityAuditor);
    await waitFor(() => expect(screen.getByText('Export')).toBeTruthy());

    await fireEvent.click(screen.getByText('Export'));
    const item = await screen.findByRole('menuitem', { name: /Export JSON Data/ });
    await fireEvent.click(item);

    await waitFor(() => {
      const toast = uiStore.toasts.at(-1);
      expect(toast?.type).toBe('error');
      expect(toast?.message).toContain('permission denied');
    });
  });
});

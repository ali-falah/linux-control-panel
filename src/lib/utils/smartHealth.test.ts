import { describe, expect, it } from 'vitest';
import { smartHealthGate, smartHealthIssueLabel, smartHealthTone } from './smartHealth';

describe('SMART health presentation', () => {
  it('labels a per-device error with the device and sanitized reason', () => {
    expect(
      smartHealthIssueLabel({
        disk_path: '/dev/sda',
        error: 'SMART data could not be read',
      }),
    ).toBe('/dev/sda: SMART data could not be read');
  });

  it('fetches only after admin mode is enabled, once per session', () => {
    let gate = smartHealthGate(false, false);
    expect(gate).toEqual({ fetch: false, loaded: false });

    gate = smartHealthGate(true, gate.loaded);
    expect(gate).toEqual({ fetch: true, loaded: true });

    gate = smartHealthGate(true, gate.loaded);
    expect(gate.fetch).toBe(false);

    gate = smartHealthGate(false, gate.loaded);
    expect(gate).toEqual({ fetch: false, loaded: false });

    gate = smartHealthGate(true, gate.loaded);
    expect(gate.fetch).toBe(true);
  });

  it('maps health statuses to display tones', () => {
    expect(smartHealthTone('PASSED')).toBe('success');
    expect(smartHealthTone('OK')).toBe('success');
    expect(smartHealthTone('FAILED')).toBe('error');
    expect(smartHealthTone('BAD')).toBe('error');
    expect(smartHealthTone('UNKNOWN')).toBe('warning');
  });
});

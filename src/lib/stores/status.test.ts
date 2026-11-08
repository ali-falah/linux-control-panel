import { beforeEach, describe, expect, it } from 'vitest';
import { statusStore } from './status.svelte.ts';

const OFF = { on: false, expires_at: null, error: null };

describe('admin mode state', () => {
  beforeEach(() => {
    statusStore.adminMode = { on: false, expiresAt: null, error: null };
    statusStore.adminNotice = null;
    statusStore.adminActionError = null;
  });

  it('starts off and reports no remaining time', () => {
    expect(statusStore.adminMode).toEqual({ on: false, expiresAt: null, error: null });
    expect(statusStore.adminModeRemaining).toBeNull();
  });

  it('counts down from the real PolicyKit expiry, not a fixed lifetime', () => {
    statusStore.applyAdminMode({
      on: true,
      expires_at: Math.floor(Date.now() / 1000) + 90,
      error: null,
    });
    expect(statusStore.adminMode.on).toBe(true);
    const remaining = statusStore.adminModeRemaining;
    expect(remaining).toBeGreaterThan(85);
    expect(remaining).toBeLessThanOrEqual(90);
  });

  it('announces an expiry once instead of letting the toggle flip silently', () => {
    statusStore.applyAdminMode({
      on: true,
      expires_at: Math.floor(Date.now() / 1000) + 5,
      error: null,
    });
    statusStore.applyAdminMode(OFF);
    expect(statusStore.adminMode.on).toBe(false);
    expect(statusStore.adminNotice).toBe('Admin mode expired');
    expect(statusStore.adminMode.error).toBeNull();
  });

  it('uses the explicit confirmation when the user switched it off', () => {
    statusStore.applyAdminMode({
      on: true,
      expires_at: Math.floor(Date.now() / 1000) + 5,
      error: null,
    });
    statusStore.applyAdminMode(OFF, 'Admin mode off');
    expect(statusStore.adminNotice).toBe('Admin mode off');
  });

  it('stays off and carries the error when enabling failed', () => {
    statusStore.applyAdminMode({ on: false, expires_at: null, error: 'Admin mode was not enabled.' });
    expect(statusStore.adminMode.on).toBe(false);
    expect(statusStore.adminMode.error).toBe('Admin mode was not enabled.');
    expect(statusStore.adminNotice).toBeNull();
  });

  it('never shows a countdown for a state that is off', () => {
    statusStore.applyAdminMode({
      on: false,
      expires_at: Math.floor(Date.now() / 1000) + 90,
      error: null,
    });
    expect(statusStore.adminModeRemaining).toBeNull();
  });
});

/**
 * systemd's `UnitFileState` covers more than "enabled" / "disabled".
 * Several states cannot be toggled with `systemctl enable|disable` at all,
 * so the UI has to present them differently instead of offering an action
 * that systemd would reject.
 */
export type UnitBootState = 'enabled' | 'enabled-runtime' | 'disabled' | 'unavailable';

export function bootEnableState(unitFileState: string | null | undefined): UnitBootState {
  switch (unitFileState) {
    case 'enabled':
      return 'enabled';
    case 'enabled-runtime':
      return 'enabled-runtime';
    case 'static':
    case 'indirect':
    case 'generated':
    case 'alias':
    case 'transient':
    case 'masked':
    case 'masked-runtime':
    case 'bad':
      return 'unavailable';
    default:
      return 'disabled';
  }
}

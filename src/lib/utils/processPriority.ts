/** Linux niceness range accepted by the System Info process inspector. */
export const PROCESS_PRIORITY_MIN = -20;
export const PROCESS_PRIORITY_MAX = 19;

export function isValidProcessPriority(value: number): boolean {
  return (
    Number.isInteger(value) &&
    value >= PROCESS_PRIORITY_MIN &&
    value <= PROCESS_PRIORITY_MAX
  );
}

export function processPriorityLabel(value: number): string {
  if (value < 0) return 'High Priority';
  if (value === 0) return 'Normal Priority';
  return 'Low Priority';
}

/** The privileged renice action is limited to the invoking user's processes. */
export function canReniceProcess(processUser: string, currentUser: string): boolean {
  return (
    processUser.trim().length > 0 &&
    currentUser.trim().length > 0 &&
    processUser.trim() === currentUser.trim()
  );
}

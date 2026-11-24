export interface ServiceUnit {
  name: string;
  load_state: string;
  active_state: string;
  sub_state: string;
  description: string;
  unit_file_state: string;
  is_protected?: boolean;
  protection_level?: string;
  protection_reason?: string;
}

export type ServiceAction =
  | 'start'
  | 'stop'
  | 'restart'
  | 'enable'
  | 'disable'
  | 'mask'
  | 'unmask'
  | 'reload';

export interface UnitDeps {
  requires: string[];
  wants: string[];
  after: string[];
  before: string[];
}

export interface AutostartEntry {
  name: string;
  exec: string;
  comment: string;
  enabled: boolean;
  file_path: string;
  icon: string | null;
}

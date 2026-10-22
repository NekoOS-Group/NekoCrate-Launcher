export interface Settings {
  microsoft_client_id: string;
  download_source: string;
  concurrency: number;
  retries: number;
  auto_java: boolean;
}
export interface Instance {
  id: string;
  name: string;
  icon?: number | null;
  versions: InstanceVersion[];
  selected_version: string | null;
  memory_mb: number;
  java_path: string | null;
  created_at: number;
  last_played: number | null;
}
export interface InstanceVersion {
  id: string;
  installed: boolean;
}
export interface Account {
  id: string;
  username: string;
  expires_at: number;
  client_id: string;
  owned: boolean;
  skin?: { url: string; slim: boolean } | null;
}
export interface Snapshot {
  root: string;
  database: {
    settings: Settings;
    instances: Instance[];
    accounts: Account[];
    selected_account: string | null;
  };
  running: string[];
}
export interface VersionManifest {
  latest: Record<string, string>;
  versions: { id: string; type: string; url: string; sha1: string }[];
}
export interface LauncherEvent {
  kind: string;
  message: string;
  instance_id: string | null;
  completed: number;
  total: number;
  bytes: number;
}
export interface Challenge {
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}
export interface Task {
  id: number;
  instance_id: string | null;
  cancellable: boolean;
  title: string;
  status: "running" | "done" | "error" | "cancelled";
  message: string;
  progress: number;
  indeterminate: boolean;
  started: string;
}

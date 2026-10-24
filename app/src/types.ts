import type { FileOrder } from "./fileOrder.ts";

export interface Settings {
  microsoft_client_id: string;
  download_source: string;
  mod_download_source: string;
  proxy_mode: string;
  proxy_url: string;
  proxy_username: string;
  proxy_password_key: string | null;
  /** Write-only change: undefined keeps the saved password, empty clears it. */
  proxy_password?: string;
  concurrency: number;
  adaptive_concurrency: boolean;
  retries: number;
  auto_java: boolean;
  language: string;
  apply_language_to_game: boolean;
}
export interface UpdateStatus {
  latest_version: string;
  available: boolean;
  download_url: string;
  updater_url: string | null;
  install_support: "development" | "package_manager" | "unbundled" | null;
}
export interface LauncherUpdateProgress {
  stage:
    | "idle"
    | "checking"
    | "downloading"
    | "verifying"
    | "ready"
    | "installing"
    | "restarting"
    | "error";
  downloaded: number;
  total: number | null;
  version: string | null;
  error: string | null;
}
export interface JavaInfo {
  path: string;
  major: number;
  version: string;
  arch?: string;
}
export type MemoryMode = "auto" | "recommended" | "minimum" | "custom";
export interface MemoryOptions {
  auto_mb: number;
  recommended_mb: number;
  minimum_mb: number;
  declared_minimum_mb: number | null;
  total_mb: number;
  available_mb: number;
}
export interface Instance {
  id: string;
  name: string;
  icon?: number | null;
  game_directory?: string | null;
  versions: InstanceVersion[];
  selected_version: string | null;
  memory_mb: number;
  memory_mode?: MemoryMode;
  java_path: string | null;
  created_at: number;
  last_played: number | null;
}
export interface InstanceVersion {
  id: string;
  minecraft_version?: string;
  loader?: { kind: string; version: string };
  installed: boolean;
  download_progress?: number;
}
export interface FabricLoaderVersion {
  version: string;
  stable: boolean;
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
  system_locale: string;
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
  total_bytes?: number;
  reset_progress?: boolean;
  step?: string;
  files?: DownloadFile[];
}
export type FileStatus =
  "queued" | "downloading" | "verifying" | "done" | "error" | "cancelled";
export interface DownloadFile {
  id: string;
  name: string;
  downloaded: number;
  total: number;
  status: FileStatus;
  cached?: boolean;
  step?: string;
}
export interface WorkflowStep {
  id: string;
  label: string;
  status: "pending" | "running" | "paused" | "done" | "error" | "cancelled";
  fileCount: number;
  filesDone: number;
}
export interface WorkflowState {
  steps: WorkflowStep[];
  files: Map<string, DownloadFile>;
  fileOrder: Map<string, FileOrder>;
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
  version_id: string | null;
  download_progress: number;
  cancellable: boolean;
  title: string;
  status: "running" | "paused" | "done" | "error" | "cancelled";
  message: string;
  progress: number;
  indeterminate: boolean;
  started: string;
  workflow: WorkflowState;
}

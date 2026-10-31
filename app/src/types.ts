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
  jdk?: boolean;
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
  launch?: import("./instanceLaunch").InstanceLaunchOptions;
  java_path: string | null;
  created_at: number;
  last_played: number | null;
}
export interface SuggestedInstance {
  path: string;
  name: string;
  source: "official" | "portable";
}
export interface InstanceVersion {
  id: string;
  name?: string | null;
  minecraft_version?: string;
  loader?: { kind: string; version: string };
  modpack?: boolean;
  installed: boolean;
  download_progress?: number;
}
export type ContentKind =
  | "mods"
  | "datapacks"
  | "resourcepacks"
  | "shaderpacks"
  | "saves"
  | "unresolved";
export type VersionContentTab = ContentKind | "libraries";
export type VersionContentCounts = Record<VersionContentTab, number>;
export interface LocalContentFile {
  file_name: string;
  name: string;
  version: string | null;
  enabled: boolean;
  size: number;
  mod_ids: string[];
  has_metadata?: boolean;
  has_config?: boolean;
  declaration?: {
    key: string;
    source: string;
    project: number | null;
    file: number | null;
    path: string | null;
    status:
      "unresolved" | "pending" | "partial" | "local" | "manual" | "skipped";
    downloaded: number;
    total: number;
    optional: boolean;
  };
}
export interface VersionContent {
  directory: string;
  files: LocalContentFile[];
}
export interface ContentLoadOptions {
  onProgress?: (content: VersionContent) => void;
  signal?: AbortSignal;
  fresh?: boolean;
}
export interface ContentMetadata {
  path: string;
  size: number | null;
  sources: {
    source: string;
    revealable: boolean;
    title: string | null;
    description: string | null;
    fields: {
      label: string;
      value: string;
      format: "translate" | "timestamp" | null;
    }[];
    dependencies: { id: string; requirement: string; relation: string }[];
    issue: string | null;
  }[];
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
    guest_name: string;
    guest_skin: string;
  };
  running: string[];
}
export interface VersionManifest {
  latest: Record<string, string>;
  versions: { id: string; type: string; url: string; sha1: string }[];
}
export interface CrashReport {
  version: string;
  time: number;
  exit: string;
  sources: string[];
  findings: {
    code: string;
    title: string;
    advice: string;
    evidence: string[];
  }[];
}
export interface LauncherEvent {
  pack_missing?: ModpackStatus;
  crash?: CrashReport;
  kind: string;
  message: string;
  instance_id: string | null;
  completed: number;
  total: number;
  bytes: number;
  total_bytes?: number;
  overall_progress?: number;
  reset_progress?: boolean;
  step?: string;
  files?: DownloadFile[];
}
export type FileStatus =
  | "queued"
  | "downloading"
  | "verifying"
  | "reusing"
  | "installing"
  | "done"
  | "error"
  | "cancelled";
export interface DownloadFile {
  error?: string;
  retryable?: boolean;
  id: string;
  name: string;
  downloaded: number;
  total: number;
  status: FileStatus;
  cached?: boolean;
  step?: string;
}
export interface WorkflowStep {
  detail?: string;
  resolved?: { completed: number; total: number };
  activity?: Record<string, number>;
  id: string;
  label: string;
  status:
    | "pending"
    | "running"
    | "paused"
    | "waiting"
    | "done"
    | "error"
    | "cancelled";
  fileCount: number;
  filesDone: number;
}
export interface WorkflowState {
  steps: WorkflowStep[];
  files: Map<string, DownloadFile>;
  fileOrder: Map<string, FileOrder>;
}
export interface Challenge {
  session_id: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}
export interface Task {
  operation?: string;
  arguments?: Record<string, unknown>;
  resource_task?: string;
  resource_pausable?: boolean;
  modpack_status?: ModpackStatus;
  id: number;
  instance_id: string | null;
  version_id: string | null;
  download_progress: number;
  cancellable: boolean;
  title: string;
  status: "running" | "paused" | "waiting" | "done" | "error" | "cancelled";
  message: string;
  progress: number;
  indeterminate: boolean;
  started: string;
  workflow: WorkflowState;
}

export interface MetadataDocument {
  id: string;
  name: string;
  kind: string;
  path: string;
  json: string | null;
  issue: string | null;
  fields: { label: string; value: string }[];
  sections: {
    label: string;
    target: VersionContentTab | null;
    note: string | null;
    entries: { name: string; detail: string | null }[];
  }[];
}
export interface VersionMetadata {
  directory: string;
  game_directory: string;
  working_directory: string | null;
  minecraft_version: string;
  loader: { kind: string; version: string } | null;
  main_class: string | null;
  java_major: number | null;
  documents: MetadataDocument[];
  libraries: { name: string; detail: string | null }[];
  issue: string | null;
}
export interface VersionStorage {
  directory: string;
  relative: string;
  exists: boolean;
  files: {
    name: string;
    relative: string;
    directory: boolean;
    symlink: boolean;
    size: number;
  }[];
}

export interface ModpackFile {
  display_name?: string | null;
  key: string;
  path: string | null;
  optional: boolean;
  size: number;
  manual_url: string | null;
  project: number | null;
  file: number | null;
}
export interface ModpackPreview {
  pack: {
    name: string;
    version: string;
    format: string;
    minecraft: string;
    loader: { kind: string; version: string } | null;
    files: ModpackFile[];
  };
}
export interface ModpackStatus {
  name: string;
  missing: ModpackFile[];
  completed: boolean;
}

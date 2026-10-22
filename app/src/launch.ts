import type { Instance } from "./types";

interface LaunchOperations {
  install: (instance: Instance) => Promise<boolean | undefined>;
  launch: (instance: Instance) => Promise<void>;
}

// Keep the original instance through installation, even if the user browses
// another instance while its files are downloading.
export async function prepareAndLaunch(
  instance: Instance,
  operations: LaunchOperations,
): Promise<void> {
  const version = instance.versions.find(
    (entry) => entry.id === instance.selected_version,
  );
  if (!version) return;
  if (!version.installed && !(await operations.install(instance))) return;
  await operations.launch(instance);
}

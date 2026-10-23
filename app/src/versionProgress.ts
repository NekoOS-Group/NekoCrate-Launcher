import type { InstanceVersion, Task } from "./types";

export function versionDownloadState(
  instanceId: string,
  version: InstanceVersion,
  tasks: readonly Task[],
) {
  const task = tasks.find(
    (entry) =>
      entry.instance_id === instanceId && entry.version_id === version.id,
  );
  return {
    progress: version.installed
      ? 100
      : Math.min(
          99,
          Math.max(
            clampProgress(task?.download_progress ?? 0),
            clampProgress(version.download_progress ?? 0),
          ),
        ),
    active: task?.status === "running",
    indeterminate: task?.status === "running" && task.indeterminate,
  };
}

function clampProgress(progress: number) {
  return Number.isFinite(progress) ? Math.min(100, Math.max(0, progress)) : 0;
}

const radius = 43.75;
const halfWidth = (radius * Math.sqrt(3)) / 2;
const vertices = [
  [50, 50 - radius],
  [50 + halfWidth, 50 - radius / 2],
  [50 + halfWidth, 50 + radius / 2],
  [50, 50 + radius],
  [50 - halfWidth, 50 + radius / 2],
  [50 - halfWidth, 50 - radius / 2],
  [50, 50 - radius],
];
export const hexagonPoints = vertices
  .slice(0, 6)
  .map((point) => point.join(","))
  .join(" ");

/** Equal-area sectors start at the top vertex and fill clockwise. */
export function hexPiePoints(progress: number) {
  const position = (clampProgress(progress) / 100) * 6;
  const index = Math.min(5, Math.floor(position));
  const fraction = position - index;
  const endpoint = vertices[index].map(
    (value, axis) => value + (vertices[index + 1][axis] - value) * fraction,
  );
  const boundary = vertices.map((point, vertex) =>
    vertex <= position ? point : endpoint,
  );
  return `50,50 ${boundary.map(([x, y]) => `${x.toFixed(4)},${y.toFixed(4)}`).join(" ")}`;
}

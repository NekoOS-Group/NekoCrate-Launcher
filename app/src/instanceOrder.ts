/** Move one entry without replacing a possibly newer list from the backend. */
export function moveInstanceBefore<T extends { id: string }>(
  items: readonly T[],
  id: string,
  beforeId: string | null,
): T[] {
  const source = items.find((item) => item.id === id);
  if (
    !source ||
    beforeId === id ||
    (beforeId !== null && !items.some((item) => item.id === beforeId))
  )
    return [...items];
  const remaining = items.filter((item) => item.id !== id);
  const index =
    beforeId === null
      ? remaining.length
      : remaining.findIndex((item) => item.id === beforeId);
  remaining.splice(index, 0, source);
  return remaining;
}

export function instanceDropTarget(
  positions: readonly { id: string; left: number; width: number }[],
  draggedId: string,
  clientX: number,
): string | null {
  return (
    positions.find(
      (item) => item.id !== draggedId && clientX < item.left + item.width / 2,
    )?.id ?? null
  );
}

export const VISIBLE_INSTANCES = 9;
/** The overflow arrow occupies one of the nine dock slots. */
export function instanceVisibleLimit(total: number) {
  return total > VISIBLE_INSTANCES ? VISIBLE_INSTANCES - 1 : VISIBLE_INSTANCES;
}

/** Row-aware hit testing uses layout coordinates, never an animating FLIP rect. */
export function instanceGridDropTarget(
  positions: readonly {
    id: string;
    left: number;
    top: number;
    width: number;
    height: number;
  }[],
  draggedId: string,
  clientX: number,
  clientY: number,
): string | null {
  if (!positions.length) return null;
  const row = positions.reduce((nearest, item) =>
    Math.abs(clientY - item.top - item.height / 2) <
    Math.abs(clientY - nearest.top - nearest.height / 2)
      ? item
      : nearest,
  );
  return (
    positions.find(
      (item) =>
        item.id !== draggedId &&
        (item.top > row.top + 1 ||
          (Math.abs(item.top - row.top) <= 1 &&
            clientX < item.left + item.width / 2)),
    )?.id ?? null
  );
}

/** Crossing the ninth slot must actually enter the chosen region. */
export function instanceZoneDropTarget(
  items: readonly { id: string }[],
  draggedId: string,
  target: string | null,
  zone: "main" | "overflow",
): string | null {
  const remaining = items.filter((item) => item.id !== draggedId);
  const index =
    target === null
      ? remaining.length
      : remaining.findIndex((item) => item.id === target);
  const bounded =
    zone === "main"
      ? Math.min(
          index < 0 ? remaining.length : index,
          instanceVisibleLimit(items.length) - 1,
        )
      : Math.max(
          index < 0 ? remaining.length : index,
          instanceVisibleLimit(items.length),
        );
  return remaining[bounded]?.id ?? null;
}

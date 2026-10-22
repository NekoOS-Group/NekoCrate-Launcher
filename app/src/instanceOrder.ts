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

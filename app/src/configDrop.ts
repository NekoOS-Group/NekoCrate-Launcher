export interface DroppedConfig {
  name: string;
  text: string;
  format: string;
}
/** Bounded preview; the saved text is always the complete original input. */
export function configDifference(before: string, after: string) {
  const a = before.split(/\r?\n/),
    b = after.split(/\r?\n/);
  let first = 0,
    end = 0;
  while (first < a.length && first < b.length && a[first] === b[first]) first++;
  while (
    end < a.length - first &&
    end < b.length - first &&
    a[a.length - end - 1] === b[b.length - end - 1]
  )
    end++;
  const from = Math.max(0, first - 2);
  return {
    same: before === after,
    before: a.slice(from, Math.min(a.length - end + 2, from + 100)).join("\n"),
    after: b.slice(from, Math.min(b.length - end + 2, from + 100)).join("\n"),
    truncated: Math.max(a.length, b.length) - end - from > 100,
  };
}

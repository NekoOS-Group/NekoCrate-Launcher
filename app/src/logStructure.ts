import type { LogRow } from "./logPresentation.ts";

/** Only explicit tree connectors (or indented list markers) imply hierarchy. */
export function logBranch(message: string) {
  const match = message.match(
    /^([\t │┃|]*)(?:[|+\\](?:-{1,3})|[├└╰╞╘][─━═]{1,3}|-(?=\s))\s+(\S[^\r\n]*)$/,
  );
  if (!match || (!match[1] && message.startsWith("- "))) return;
  let indent = 0;
  for (const char of match[1]) indent += char === "\t" ? 4 - (indent % 4) : 1;
  return { indent, text: match[2] };
}

export function logSeparator(message: string) {
  return /^\s*([-=_*─━])\1{5,}\s*$/.test(message);
}

/** A conservative template: vary quoted code identifiers, never prose/numbers. */
export function logMessageTemplate(message: string) {
  if (message.includes("\n")) return;
  let replaced = 0;
  const template = message.replace(
    /(['"`])[A-Za-z_$][\w$]*(?:\.[\w$]+)+\1/g,
    () => {
      replaced++;
      return "‹identifier›";
    },
  );
  if (
    replaced > 0 &&
    replaced <= 3 &&
    template.length >= 48 &&
    template.length >= message.length * 0.45
  )
    return template;
}

export type LogNode = {
  id: number;
  row: LogRow;
  text: string;
  time: string;
  count: number;
  continuation: boolean;
  indent?: number;
  depth: number;
  children: LogNode[];
  parent?: LogNode;
  descendants: number;
  collapsed: boolean;
};

/** Cache text parsing and node identities; folding never changes the raw log. */
export function createLogOutline() {
  let nextId = -1;
  let roots: LogNode[] = [],
    all: LogNode[] = [];
  let head: LogNode | undefined;
  let stream: LogRow | undefined;
  let stack: LogNode[] = [];
  let processed = 0;
  let first: LogRow | undefined, last: LogRow | undefined, filterKey: unknown;
  let checkpoint: {
    length: number;
    stack: LogNode[];
    head?: LogNode;
    stream?: LogRow;
  };
  const cache = new WeakMap<
    LogRow,
    { text: string; variants: number; nodes: LogNode[] }
  >();
  function parts(row: LogRow): LogNode[] {
    const text = row.message;
    const variants = row.variants?.length ?? 0;
    let cached = cache.get(row);
    if (!cached || cached.text !== text || cached.variants !== variants) {
      const lines = row.message.split(/\r?\n/);
      const enclosed =
        lines.length > 2 &&
        logSeparator(lines[0]) &&
        logSeparator(lines.at(-1)!);
      const block = row.block || enclosed;
      const structured = block || lines.some((line) => logBranch(line));
      const entries = row.variants
        ? row.variants.map((variant) => variant.message)
        : block
          ? (() => {
              const [title, ...body] = (
                enclosed ? lines.slice(1, -1).join("\n") : row.message
              )
                .trim()
                .split("\n");
              return [
                title,
                ...body
                  .join("\n")
                  .trim()
                  .split(/\n\s*\n/),
              ].filter(Boolean);
            })()
          : structured
            ? lines
            : [row.message];
      const nodes = entries.map((message, index) => {
        const branch = row.variants || block ? undefined : logBranch(message);
        return {
          id: cached?.nodes[index]?.id ?? (index ? nextId-- : row.id),
          row,
          text: branch?.text ?? message,
          time: row.time,
          count: 1,
          continuation: index > 0 && !row.variants,
          indent: branch?.indent,
          depth: 0,
          children: [],
          descendants: 0,
          collapsed: !!(row.variants || block),
        } satisfies LogNode;
      });
      cached = { text, variants, nodes };
      cache.set(row, cached);
    }
    for (const [index, node] of cached.nodes.entries()) {
      node.children = [];
      node.depth = 0;
      node.descendants = 0;
      node.parent = undefined;
      node.time = row.variants?.[index]?.time ?? row.time;
      node.count = index ? (row.variants?.[index]?.count ?? 1) : row.count;
    }
    return cached.nodes;
  }
  return {
    build(rows: readonly LogRow[], key?: unknown) {
      let start = 0;
      if (
        processed &&
        key === filterKey &&
        rows.length >= processed &&
        rows[0] === first &&
        rows[processed - 1] === last
      ) {
        // Only the former tail can grow into a block/group. Rewind that row,
        // retaining all historical parsing, hierarchy and node identities.
        for (let index = all.length - 1; index >= checkpoint.length; index--) {
          const node = all[index];
          (node.parent?.children || roots).pop();
          for (let parent = node.parent; parent; parent = parent.parent)
            parent.descendants--;
        }
        all.length = checkpoint.length;
        stack = checkpoint.stack;
        head = checkpoint.head;
        stream = checkpoint.stream;
        start = processed - 1;
      } else {
        roots = [];
        all = [];
        stack = [];
        head = undefined;
        stream = undefined;
      }
      for (let index = start; index < rows.length; index++) {
        if (index === rows.length - 1)
          checkpoint = {
            length: all.length,
            stack: stack.slice(),
            head,
            stream,
          };
        const row = rows[index];
        if (
          stream?.scope !== row.scope ||
          stream.filterSource !== row.filterSource ||
          stream.filterLevel !== row.filterLevel
        ) {
          stack.length = 0;
          head = undefined;
        }
        stream = row;
        const nodes = parts(row);
        for (const node of nodes) {
          let parent: LogNode | undefined;
          if (node !== nodes[0] && nodes[0].collapsed) {
            parent = nodes[0];
          } else if (node.indent !== undefined) {
            while (stack.length && stack.at(-1)!.indent! >= node.indent)
              stack.pop();
            parent =
              stack.at(-1) ||
              (head?.row.group === row.group ? head : undefined);
            stack.push(node);
          } else {
            stack.length = 0;
            head = node;
          }
          node.depth = parent ? parent.depth + 1 : 0;
          node.parent = parent;
          (parent?.children || roots).push(node);
          for (let ancestor = parent; ancestor; ancestor = ancestor.parent)
            ancestor.descendants++;
          all.push(node);
        }
      }
      processed = rows.length;
      first = rows[0];
      last = rows.at(-1);
      filterKey = key;
      return roots.slice();
    },
  };
}

export function visibleLogNodes(
  roots: readonly LogNode[],
  expanded: ReadonlyMap<number, boolean>,
  searching = false,
) {
  const visible: LogNode[] = [];
  const pending = [...roots].reverse();
  while (pending.length) {
    const node = pending.pop()!;
    visible.push(node);
    if (expanded.get(node.id) ?? (searching || !node.collapsed))
      for (let index = node.children.length - 1; index >= 0; index--)
        pending.push(node.children[index]);
  }
  return visible;
}

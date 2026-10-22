import type { BackdropKind } from "./flowGraph";

export const backdropThemes: {
  id: string;
  name: string;
  color: string;
  kind: BackdropKind;
  pattern: string;
  mark: string;
}[] = [
  {
    id: "tide",
    name: "海蓝",
    color: "#80ced7",
    kind: "honeycomb",
    pattern: "蜂巢",
    mark: "M6 2 10.33 4.5v5L6 12 1.67 9.5v-5Z",
  },
  {
    id: "sage",
    name: "青苔",
    color: "#a5d6ad",
    kind: "chunks",
    pattern: "方块矩阵",
    mark: "M1.5 2.5h9v9h-9ZM5 2.5v9M1.5 6h9",
  },
  {
    id: "rose",
    name: "蔷薇",
    color: "#e6abc4",
    kind: "diamonds",
    pattern: "菱形晶格",
    mark: "M6 1.5 11.5 7 6 12.5.5 7ZM6 1.5v11",
  },
  {
    id: "iris",
    name: "鸢尾",
    color: "#beb2ef",
    kind: "facets",
    pattern: "三角切面",
    mark: "M6 2 11.5 11.5H.5Z",
  },
  {
    id: "slate",
    name: "石墨",
    color: "#b7c8db",
    kind: "circuit",
    pattern: "切角电路",
    mark: "M3.928932 2H8.071068L11 4.928932V9.071068L8.071068 12H3.928932L1 9.071068V4.928932Z",
  },
];

export function backdropForTheme(theme: string) {
  return (
    backdropThemes.find((candidate) => candidate.id === theme) ||
    backdropThemes[0]!
  );
}

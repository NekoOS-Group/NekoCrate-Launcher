import oak from "./assets/blocks/oak_planks.png";
import cherry from "./assets/blocks/cherry_planks.png";
import grassTop from "./assets/blocks/grass_block_top.png";
import grassSide from "./assets/blocks/grass_block_side.png";
import grassOverlay from "./assets/blocks/grass_block_side_overlay.png";
import ice from "./assets/blocks/ice.png";
import bricks from "./assets/blocks/bricks.png";
import slime from "./assets/blocks/slime_block.png";
import stone from "./assets/blocks/stone_bricks.png";
import craftingTop from "./assets/blocks/crafting_table_top.png";
import craftingSide from "./assets/blocks/crafting_table_side.png";
import craftingFront from "./assets/blocks/crafting_table_front.png";
import sandstone from "./assets/blocks/sandstone.png";
import sandstoneTop from "./assets/blocks/sandstone_top.png";
import quartz from "./assets/blocks/quartz_block_side.png";
import quartzTop from "./assets/blocks/quartz_block_top.png";
import prismarine from "./assets/blocks/prismarine_bricks.png";
import copper from "./assets/blocks/cut_copper.png";
import oxidizedCopper from "./assets/blocks/oxidized_cut_copper.png";
import amethyst from "./assets/blocks/amethyst_block.png";
import hay from "./assets/blocks/hay_block_side.png";
import hayTop from "./assets/blocks/hay_block_top.png";

export interface InstanceIconDefinition {
  id: number;
  name: string;
  faces?: {
    top: string;
    side: string;
    front?: string;
    tint?: readonly [number, number, number];
    sideOverlay?: string;
    inset?: boolean;
  };
}

// IDs are persisted. Replace artwork in place; never reorder existing entries.
// Keep the core's INSTANCE_ICON_COUNT in sync when extending this catalog.
export const instanceIcons: InstanceIconDefinition[] = [
  {
    id: 0,
    name: "草方块",
    faces: {
      top: grassTop,
      side: grassSide,
      sideOverlay: grassOverlay,
      tint: [124, 189, 107],
    },
  },
  { id: 1, name: "冰块", faces: { top: ice, side: ice } },
  { id: 2, name: "红砖", faces: { top: bricks, side: bricks } },
  { id: 3, name: "苔石" },
  { id: 4, name: "橡木木板", faces: { top: oak, side: oak } },
  { id: 5, name: "樱花木板", faces: { top: cherry, side: cherry } },
  {
    id: 6,
    name: "史莱姆方块",
    faces: { top: slime, side: slime, inset: true },
  },
  { id: 7, name: "石砖", faces: { top: stone, side: stone } },
  {
    id: 8,
    name: "工作台",
    faces: { top: craftingTop, side: craftingSide, front: craftingFront },
  },
  { id: 9, name: "砂岩", faces: { top: sandstoneTop, side: sandstone } },
  { id: 10, name: "石英块", faces: { top: quartzTop, side: quartz } },
  { id: 11, name: "海晶石砖", faces: { top: prismarine, side: prismarine } },
  { id: 12, name: "切制铜块", faces: { top: copper, side: copper } },
  {
    id: 13,
    name: "氧化切制铜块",
    faces: { top: oxidizedCopper, side: oxidizedCopper },
  },
  { id: 14, name: "紫水晶块", faces: { top: amethyst, side: amethyst } },
  { id: 15, name: "干草块", faces: { top: hayTop, side: hay } },
];

export function resolveInstanceIcon(
  icon: number | null | undefined,
  id: string,
) {
  if (instanceIcons.some((candidate) => candidate.id === icon)) return icon!;
  return (Number.parseInt(id.replaceAll("-", "").slice(0, 8), 16) || 0) % 4;
}

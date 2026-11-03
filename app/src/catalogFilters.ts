import { locale } from "./i18n.ts";

const categories: Record<string, string> = {
  adventure: "冒险",
  "adventure and rpg": "冒险与 RPG",
  challenging: "挑战",
  combat: "战斗",
  "combat / pvp": "战斗 / PvP",
  exploration: "探索",
  "kitchen-sink": "综合",
  "kitchen sink": "综合",
  lightweight: "轻量",
  magic: "魔法",
  multiplayer: "多人",
  optimization: "性能优化",
  quests: "任务",
  "quests / progression": "任务 / 进度",
  technology: "科技",
  tech: "科技",
  vanilla_like: "原版风格",
  "vanilla+": "原版增强",
  horror: "恐怖",
  skyblock: "空岛",
  "sci-fi": "科幻",
  hardcore: "硬核",
  small: "小型",
  "extra large": "超大型",
  "mini game": "小游戏",
  "multiplayer server": "多人服务器",
  "ftb official pack": "FTB 官方整合包",
  "map based": "地图玩法",
  "server utility": "服务器工具",
};
export function categoryLabel(name: string): string {
  if (locale.value === "zh-CN" && categories[name.toLowerCase()])
    return categories[name.toLowerCase()];
  return name.replaceAll("_", " ").replace(/^./, (c) => c.toUpperCase());
}

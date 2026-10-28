import steve from "./assets/steve.png";
import cirno from "./assets/cirno.png";
import steveInfo from "./assets/steve.provenance.json";
import cirnoInfo from "./assets/cirno.provenance.json";
import type { BuiltinSkin } from "./skinLibrary";

/** App assets: available without an account, network, or skin cache. */
export const builtinSkins = [
  {
    id: "builtin:steve",
    name: "steve",
    slim: false,
    texture: steve,
    sha256: steveInfo.asset_sha256,
    builtin: true,
  },
  {
    id: "builtin:cirno",
    name: "cirno",
    slim: true,
    texture: cirno,
    sha256: cirnoInfo.asset_sha256,
    builtin: true,
  },
] as const satisfies readonly BuiltinSkin[];

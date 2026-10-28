import type { Account } from "./types";

export type SavedSkin = {
  id: string;
  name: string;
  slim: boolean;
  texture_url?: string;
};
export type SkinChoice = SavedSkin & {
  account?: string;
  textureKey?: string;
  builtin?: boolean;
  texture?: string;
  users?: { id: string; name: string }[];
};
export type BuiltinSkin = SavedSkin & {
  builtin: true;
  texture: string;
  sha256: string;
};

export function isGuestSkin(item: SkinChoice, id: string): boolean {
  return (
    item.id === id || !!item.users?.some((user) => `account:${user.id}` === id)
  );
}

/** Keep the bundled name/texture and attach account ownership to that card. */
export function mergeBuiltinAccountSkins(
  builtins: readonly BuiltinSkin[],
  accounts: readonly Account[],
  saved: readonly SavedSkin[] = [],
): SkinChoice[] {
  const choices: SkinChoice[] = [...builtins, ...saved].map((skin) => ({
    ...skin,
    users: [],
  }));
  for (const account of accounts) {
    const hash = account.skin?.url
      .match(
        /^https?:\/\/textures\.minecraft\.net\/texture\/([a-f\d]{32,64})$/i,
      )?.[1]
      .toLowerCase();
    const index = choices.findIndex((skin) =>
      account.skin
        ? (skin.texture_url === account.skin.url ||
            (skin.builtin
              ? builtins.find((s) => s.id === skin.id)?.sha256
              : skin.id) === hash) &&
          skin.slim === !!account.skin.slim
        : skin.id === "builtin:steve",
    );
    const user = { id: account.id, name: account.username };
    if (index >= 0) {
      choices[index]!.users!.push(user);
    } else {
      choices.push({
        id: `account:${account.id}`,
        account: account.id,
        name: account.username,
        slim: !!account.skin?.slim,
        textureKey: account.skin?.url || "",
        texture_url: account.skin?.url,
        users: [user],
      });
    }
  }
  return choices;
}

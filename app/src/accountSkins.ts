import type { Account } from "./types";

/** Official texture URLs are content-addressed. Share pending IPC/downloads
 * between the avatar and model; token refreshes do not change the texture. */
export function createAccountSkinLoader(
  fetch: (id: string) => Promise<string | null>,
) {
  const textures = new Map<string, Promise<string | null>>();
  return (account: Account): Promise<string | null> => {
    if (!account.skin) return Promise.resolve(null);
    const key = account.skin.url;
    const cached = textures.get(key);
    if (cached) {
      textures.delete(key);
      textures.set(key, cached);
      return cached;
    }
    const forget = () => {
      if (textures.get(key) === pending) textures.delete(key);
    };
    const pending = fetch(account.id).then(
      (texture) => {
        if (!texture) forget();
        return texture;
      },
      (error) => {
        forget();
        throw error;
      },
    );
    textures.set(key, pending);
    while (textures.size > 12) textures.delete(textures.keys().next().value!);
    return pending;
  };
}

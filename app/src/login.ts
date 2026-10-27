import type { Challenge } from "./types";

interface LoginOperations<T> {
  begin: () => Promise<Challenge>;
  showChallenge: (challenge: Challenge) => void;
  copyCode: (code: string) => Promise<void>;
  copyFailed: () => void;
  openBrowser: (url: string) => Promise<void>;
  browserFailed: (error: unknown) => void;
  waitForLogin: (challenge: Challenge) => Promise<T>;
  isCurrent?: () => boolean;
}

export async function signInWithMicrosoft<T>(
  operations: LoginOperations<T>,
): Promise<T | undefined> {
  const challenge = await operations.begin();
  if (operations.isCurrent?.() === false) return;
  operations.showChallenge(challenge);
  // Poll while the browser opens, so cancellation is already available.
  // A browser failure must leave the device code usable for manual sign-in.
  let active = true;
  const authorization = operations.waitForLogin(challenge);
  // OS browser/clipboard completion is independent of successful authorization.
  // Do not keep an authenticated account waiting for either callback.
  void (async () => {
    try {
      await operations.copyCode(challenge.user_code);
    } catch {
      if (active && operations.isCurrent?.() !== false) operations.copyFailed();
    }
    if (!active || operations.isCurrent?.() === false) return;
    try {
      await operations.openBrowser(challenge.verification_uri);
    } catch (error) {
      if (active && operations.isCurrent?.() !== false)
        operations.browserFailed(error);
    }
  })();
  try {
    return await authorization;
  } finally {
    active = false;
  }
}

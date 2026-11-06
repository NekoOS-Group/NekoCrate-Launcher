import type { Challenge } from "./types";

interface LoginOperations<T> {
  begin: () => Promise<Challenge>;
  showChallenge: (challenge: Challenge) => void;
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
  // Start receiving the callback before opening the browser.
  let active = true;
  const authorization = operations.waitForLogin(challenge);
  // OS browser completion is independent of successful authorization.
  // Do not keep an authenticated account waiting for that callback.
  void (async () => {
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

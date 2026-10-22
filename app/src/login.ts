import type { Challenge } from "./types";

interface LoginOperations {
  begin: () => Promise<Challenge>;
  showChallenge: (challenge: Challenge) => void;
  openBrowser: (url: string) => Promise<void>;
  browserFailed: (error: unknown) => void;
  waitForLogin: () => Promise<unknown>;
}

export async function signInWithMicrosoft(
  operations: LoginOperations,
): Promise<void> {
  const challenge = await operations.begin();
  operations.showChallenge(challenge);
  // Poll while the browser opens, so cancellation is already available.
  // A browser failure must leave the device code usable for manual sign-in.
  let active = true;
  try {
    await Promise.all([
      operations.waitForLogin(),
      (async () => {
        try {
          await operations.openBrowser(challenge.verification_uri);
        } catch (error) {
          if (active) operations.browserFailed(error);
        }
      })(),
    ]);
  } finally {
    active = false;
  }
}

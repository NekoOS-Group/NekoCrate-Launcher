import assert from "node:assert/strict";
import test from "node:test";
import { createAccountSkinLoader } from "../src/accountSkins.ts";

const account = { id: "player", skin: { url: "texture-a", slim: false } };
test("avatar and model share an in-flight texture and reuse it across account refreshes", async () => {
  let resolve,
    calls = 0;
  const load = createAccountSkinLoader(async () => {
    calls++;
    return new Promise((done) => {
      resolve = done;
    });
  });
  const model = load(account),
    avatar = load(account);
  assert.equal(calls, 1);
  resolve("texture-data");
  assert.deepEqual(await Promise.all([model, avatar]), [
    "texture-data",
    "texture-data",
  ]);
  assert.equal(await load({ ...account, expires_at: 9999 }), "texture-data");
  assert.equal(calls, 1);
  const changed = load({ ...account, skin: { url: "texture-b" } });
  assert.equal(calls, 2);
  resolve("new-data");
  assert.equal(await changed, "new-data");
});

test("failed/empty skins can retry, missing skins avoid IPC and the cache is bounded", async () => {
  let calls = 0;
  const load = createAccountSkinLoader(async () => {
    calls++;
    if (calls === 1) throw new Error("offline");
    return calls === 2 ? null : "texture-data";
  });
  assert.equal(await load({ id: "steve" }), null);
  assert.equal(calls, 0);
  await assert.rejects(load(account), /offline/);
  assert.equal(await load(account), null);
  assert.equal(await load(account), "texture-data");
  assert.equal(calls, 3);
  for (let i = 0; i < 12; i++)
    await load({ ...account, skin: { url: `other-${i}` } });
  await load(account);
  assert.equal(
    calls,
    16,
    "old textures are evicted instead of accumulating indefinitely",
  );
});

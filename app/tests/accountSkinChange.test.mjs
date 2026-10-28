import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const account = {
  id: "first",
  username: "First",
  owned: true,
  expires_at: 9999999999,
  client_id: "fixture",
  skin: null,
};

test("clicks use the selected identity; successful uploads refresh the model without overwriting guest choice", async () => {
  const f = await settingsFixture();
  try {
    f.handlers.resource_manager = ({ action }) => {
      assert.equal(action.kind, "set_guest_skin");
      return action.id;
    };
    assert.equal(await f.context.chooseSkin("builtin:cirno"), null);
    assert.equal(f.context.guestSkinId.value, "builtin:cirno");
    f.state.database.accounts = [account];
    f.state.database.selected_account = account.id;
    f.state.database.guest_skin = "builtin:cirno";
    await f.context.sync();
    const stale = deferred(),
      old = f.fixture.snapshot();
    f.handlers.snapshot = () => stale.promise;
    const syncing = f.context.sync();
    const updated = { ...account, skin: { url: "new-texture", slim: true } };
    f.handlers.resource_manager = ({ action }) => {
      assert.deepEqual(action, {
        kind: "apply_account_skin",
        account: "first",
        id: "builtin:steve",
      });
      return updated;
    };
    f.handlers.account_skin = ({ id }) => {
      assert.equal(id, "first");
      return [1, 2, 3];
    };
    assert.deepEqual(await f.context.chooseSkin("builtin:steve"), updated);
    stale.resolve(old);
    await syncing;
    await flush();
    assert.equal(f.context.selectedAccount.value.skin.url, "new-texture");
    assert.equal(f.context.skinTexture.value, "data:image/png;base64,AQID");
    assert.equal(f.context.skinSlim.value, true);
    assert.equal(f.context.guestSkinId.value, "builtin:cirno");
  } finally {
    f.dispose();
  }
});

test("an in-flight upload stays bound to its account and cannot switch the active identity", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.accounts = [
      account,
      { ...account, id: "second", username: "Second" },
    ];
    f.state.database.selected_account = account.id;
    await f.context.sync();
    const pending = deferred();
    f.handlers.resource_manager = ({ action }) => {
      assert.equal(action.account, "first");
      return pending.promise;
    };
    f.handlers.select_account = () => true;
    f.handlers.account_skin = () => [1, 2, 3];
    const changing = f.context.chooseSkin("builtin:cirno");
    await flush();
    await f.context.chooseIdentity("second");
    pending.resolve({ ...account, skin: { url: "new-texture", slim: true } });
    await changing;
    await flush();
    assert.equal(f.context.selectedAccount.value.id, "second");
    assert.equal(f.context.selectedAccount.value.skin, null);
    assert.equal(
      f.context.snapshot.value.database.accounts[0].skin.url,
      "new-texture",
    );
    assert.equal(f.context.skinTexture.value, null);
  } finally {
    f.dispose();
  }
});

test("queued uploads keep their original target; a rejected upload preserves the skin and permits retry", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.accounts = [account, { ...account, id: "second" }];
    f.state.database.selected_account = account.id;
    await f.context.sync();
    const pending = deferred(),
      requests = [];
    f.handlers.resource_manager = ({ action }) => {
      requests.push(action);
      if (requests.length === 1) return pending.promise;
      return {
        ...account,
        id: action.account,
        skin: { url: "texture", slim: true },
      };
    };
    f.handlers.select_account = () => true;
    f.handlers.account_skin = () => [1, 2, 3];
    const first = f.context.chooseSkin("builtin:cirno");
    const rejected = assert.rejects(first, /network fixture/);
    const next = f.context.chooseSkin("builtin:steve");
    await f.context.chooseIdentity("second");
    await flush();
    assert.equal(requests.length, 1);
    pending.reject(Error("network fixture"));
    await rejected;
    await next;
    assert.deepEqual(
      requests.map((r) => r.account),
      ["first", "first"],
    );
    assert.equal(f.context.selectedAccount.value.id, "second");
    f.handlers.resource_manager = () => {
      throw Error("rate limit fixture");
    };
    await assert.rejects(f.context.chooseSkin("builtin:cirno"), /rate limit/);
    assert.equal(f.context.selectedAccount.value.skin, null);
    assert.equal(f.context.guestSkinId.value, "builtin:steve");
  } finally {
    f.dispose();
  }
});

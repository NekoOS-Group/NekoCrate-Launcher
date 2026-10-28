import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const account = {
  id: "microsoft",
  username: "Neko",
  owned: true,
  expires_at: 9999999999,
  client_id: "fixture",
  skin: null,
};

test("saved Microsoft accounts can switch to guest; late snapshots and skins cannot restore the old identity", async () => {
  const f = await settingsFixture();
  try {
    const skin = deferred();
    f.state.database.accounts = [
      { ...account, skin: { url: "texture", slim: true } },
    ];
    f.state.database.selected_account = account.id;
    f.handlers.account_skin = () => skin.promise;
    await f.context.sync();
    await flush();
    const stale = deferred(),
      old = f.fixture.snapshot();
    f.handlers.snapshot = () => stale.promise;
    const syncing = f.context.sync();
    f.handlers.select_account = ({ id }) => {
      assert.equal(id, null);
      f.state.database.selected_account = id;
      return true;
    };
    f.context.utility.value = "accounts";
    await f.context.chooseIdentity(null);
    assert.equal(f.context.utility.value, "accounts");
    stale.resolve(old);
    skin.resolve([1, 2, 3]);
    await syncing;
    await flush();
    assert.equal(f.context.snapshot.value.database.selected_account, null);
    assert.equal(f.context.snapshot.value.database.accounts.length, 1);
    assert.equal(f.context.skinTexture.value, null);
    assert.equal(f.context.skinLoading.value, false);
  } finally {
    f.dispose();
  }
});

test("guest rename persists without disabling the next account click or being overwritten by an old snapshot", async () => {
  const f = await settingsFixture();
  try {
    f.state.database.accounts = [account];
    await f.context.sync();
    const saving = deferred(),
      stale = deferred(),
      old = f.fixture.snapshot();
    f.handlers.snapshot = () => stale.promise;
    const syncing = f.context.sync();
    f.handlers.set_guest_name = ({ name }) => {
      assert.equal(name, "Local_Neko");
      return saving.promise;
    };
    f.handlers.select_account = () => true;
    f.context.guestNameDraft.value = " Local_Neko ";
    const write = f.context.saveGuestName();
    assert.equal(f.context.busy.value, false);
    assert.equal(f.context.guestNameSaving.value, true);
    await f.context.chooseAccount(account);
    saving.resolve("Local_Neko");
    await write;
    stale.resolve(old);
    await syncing;
    await flush();
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      account.id,
    );
    assert.equal(f.context.guestName.value, "Local_Neko");
    assert.equal(f.context.guestNameDraft.value, "Local_Neko");
    assert.equal(f.context.guestNameSaving.value, false);
    await f.context.chooseIdentity(null);
    assert.equal(f.context.guestName.value, "Local_Neko");
  } finally {
    f.dispose();
  }
});

test("invalid or failed guest renames retain the last saved identity; failed writes can retry", async () => {
  const f = await settingsFixture();
  try {
    for (const name of [
      "",
      " ",
      "中文",
      "has space",
      "has/slash",
      "abcdefghijklmnopq",
    ]) {
      f.context.guestNameDraft.value = name;
      await f.context.saveGuestName();
      assert.match(f.context.error.value, /1–16/);
    }
    assert.equal(
      f.calls.some((c) => c.op === "set_guest_name"),
      false,
    );
    f.handlers.set_guest_name = () => {
      throw Error("disk full");
    };
    f.context.guestNameDraft.value = "Local_Neko";
    await f.context.saveGuestName();
    assert.equal(f.context.guestName.value, "Player");
    assert.match(f.context.error.value, /disk full/);
    f.handlers.set_guest_name = ({ name }) => name;
    await f.context.saveGuestName();
    assert.equal(f.context.guestName.value, "Local_Neko");
    const before = f.calls.length;
    await f.context.saveGuestName();
    assert.equal(f.calls.length, before);
  } finally {
    f.dispose();
  }
});

test("game launch waits for the name write triggered by leaving the editor", async () => {
  const f = await settingsFixture();
  try {
    const saving = deferred();
    f.handlers.set_guest_name = () => saving.promise;
    f.handlers.launch = () => true;
    f.context.guestNameDraft.value = "Local_Neko";
    const write = f.context.saveGuestName();
    const launch = f.context.launch(
      f.context.snapshot.value.database.instances[0],
    );
    await flush();
    assert.equal(
      f.calls.some((c) => c.op === "launch"),
      false,
    );
    f.state.database.guest_name = "Local_Neko";
    saving.resolve("Local_Neko");
    await write;
    await launch;
    assert.equal(
      f.calls.some((c) => c.op === "launch"),
      true,
    );
    assert.equal(f.context.guestName.value, "Local_Neko");
  } finally {
    f.dispose();
  }
});

test("guest skin changes update the home model and survive stale snapshots and restart", async () => {
  const f = await settingsFixture();
  try {
    const stale = deferred(),
      old = f.fixture.snapshot();
    f.handlers.snapshot = () => stale.promise;
    const syncing = f.context.sync();
    f.handlers.resource_manager = ({ action }) => {
      assert.equal(action.kind, "set_guest_skin");
      f.state.database.guest_skin = action.id;
      return action.id;
    };
    await f.context.chooseGuestSkin("builtin:cirno");
    stale.resolve(old);
    await syncing;
    await flush();
    assert.equal(f.context.guestSkinId.value, "builtin:cirno");
    assert.equal(f.context.skinTexture.value, "bundled:cirno");
    assert.equal(f.context.skinSlim.value, true);
    assert.equal(f.context.snapshot.value.database.selected_account, null);
  } finally {
    f.dispose();
  }
  const reopened = await settingsFixture();
  try {
    reopened.state.database.guest_skin = "builtin:cirno";
    await reopened.context.sync();
    await flush();
    assert.equal(reopened.context.skinTexture.value, "bundled:cirno");
    assert.equal(reopened.context.skinSlim.value, true);
  } finally {
    reopened.dispose();
  }
});

test("guest skin choice does not replace a Microsoft account or its texture", async () => {
  const f = await settingsFixture();
  try {
    const microsoft = { ...account, skin: { url: "texture", slim: false } };
    f.state.database.accounts = [microsoft];
    f.state.database.selected_account = microsoft.id;
    f.handlers.account_skin = () => [1, 2, 3];
    f.handlers.select_account = () => true;
    f.handlers.resource_manager = ({ action }) => action.id;
    await f.context.sync();
    await flush();
    const original = f.context.skinTexture.value;
    await f.context.chooseGuestSkin("builtin:cirno");
    await flush();
    assert.equal(
      f.context.snapshot.value.database.selected_account,
      microsoft.id,
    );
    assert.equal(f.context.skinTexture.value, original);
    assert.equal(f.context.guestTexture.value, "bundled:cirno");
    await f.context.chooseIdentity(null);
    await flush();
    assert.equal(f.context.skinTexture.value, "bundled:cirno");
    assert.equal(f.context.skinSlim.value, true);
    await f.context.chooseAccount(microsoft);
    await flush();
    assert.equal(f.context.skinTexture.value, original);
    assert.equal(f.context.skinSlim.value, false);
  } finally {
    f.dispose();
  }
});

test("rapid selections write in order, errors preserve the last saved skin, and late textures cannot win", async () => {
  const f = await settingsFixture();
  try {
    const first = deferred(),
      texture = deferred();
    const writes = [];
    f.handlers.resource_manager = ({ action }) => {
      if (action.kind === "guest_skin") return texture.promise;
      writes.push(action.id);
      return writes.length === 1 ? first.promise : action.id;
    };
    const a = f.context.chooseGuestSkin("builtin:cirno");
    const b = f.context.chooseGuestSkin("builtin:steve");
    await flush();
    assert.deepEqual(writes, ["builtin:cirno"]);
    first.resolve("builtin:cirno");
    await Promise.all([a, b]);
    await flush();
    assert.deepEqual(writes, ["builtin:cirno", "builtin:steve"]);
    assert.equal(f.context.guestSkinId.value, "builtin:steve");
    const local = "a".repeat(64);
    f.state.database.guest_skin = local;
    await f.context.sync();
    await flush();
    await f.context.chooseGuestSkin("builtin:cirno");
    texture.resolve({
      skin: { id: local, name: "Local", slim: false },
      bytes: [4, 5, 6],
    });
    await flush();
    assert.equal(f.context.skinTexture.value, "bundled:cirno");
    assert.equal(f.context.skinSlim.value, true);
    f.handlers.resource_manager = () => {
      throw Error("disk full");
    };
    await assert.rejects(
      f.context.chooseGuestSkin("builtin:steve"),
      /disk full/,
    );
    assert.equal(f.context.guestSkinId.value, "builtin:cirno");
  } finally {
    f.dispose();
  }
});

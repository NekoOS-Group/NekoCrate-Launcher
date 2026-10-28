import assert from "node:assert/strict";
import test from "node:test";
import {
  settingsFixture,
  deferred,
  flush,
} from "./helpers/appSettingsFixture.mjs";

const account = {
  id: "first",
  username: "Original",
  owned: true,
  expires_at: 9999999999,
  client_id: "fixture",
  skin: null,
};
async function setup() {
  const f = await settingsFixture();
  f.state.database.accounts = [
    account,
    { ...account, id: "second", username: "Other" },
  ];
  f.state.database.selected_account = "second";
  await f.context.sync();
  f.context.utility.value = "accounts";
  await f.context.editAccountName(account);
  return f;
}

test("editing does not rename or select the account; successful explicit submit survives stale snapshots", async () => {
  const f = await setup();
  try {
    assert.equal(f.context.accountNameEditing.value, "first");
    assert.equal(f.context.selectedAccount.value.id, "second");
    assert.equal(
      f.calls.some((c) => c.op === "rename_account"),
      false,
    );
    const stale = deferred(),
      old = f.fixture.snapshot(),
      pending = deferred();
    f.handlers.snapshot = () => stale.promise;
    const syncing = f.context.sync();
    f.handlers.rename_account = ({ id, name }) => {
      assert.equal(id, "first");
      assert.equal(name, "New_Name");
      return pending.promise;
    };
    f.context.accountNameDraft.value = " New_Name ";
    const saving = f.context.saveAccountName();
    assert.equal(f.context.busy.value, false);
    assert.equal(f.context.accountNameSaving.value, true);
    assert.equal(
      f.context.snapshot.value.database.accounts[0].username,
      "Original",
    );
    pending.resolve({ ...account, username: "New_Name" });
    await saving;
    stale.resolve(old);
    await syncing;
    assert.equal(
      f.context.snapshot.value.database.accounts[0].username,
      "New_Name",
    );
    assert.equal(f.context.selectedAccount.value.id, "second");
    assert.equal(f.context.guestName.value, "Player");
    assert.equal(f.context.accountNameEditing.value, null);
    assert.equal(f.context.accountNameSaving.value, false);
  } finally {
    f.dispose();
  }
});

test("blur and Escape discard an unsubmitted edit; invalid names never reach IPC", async () => {
  const f = await setup();
  try {
    const outside = {
      currentTarget: { contains: () => false },
      relatedTarget: null,
    };
    f.context.accountNameDraft.value = "Changed";
    f.context.blurAccountName(outside);
    assert.equal(f.context.accountNameEditing.value, null);
    await f.context.editAccountName(account);
    f.context.accountNameDraft.value = "Changed";
    f.context.cancelAccountNameEdit();
    await f.context.editAccountName(account);
    f.context.blurAccountName({
      currentTarget: { contains: () => true },
      relatedTarget: {},
    });
    assert.equal(
      f.context.accountNameEditing.value,
      account.id,
      "Tab to confirm stays inside the editor",
    );
    for (const name of [
      "",
      "ab",
      "中文名称",
      "has space",
      "has/slash",
      "abcdefghijklmnopq",
    ]) {
      f.context.accountNameDraft.value = name;
      await f.context.saveAccountName();
      assert.match(f.context.visibleError.value, /3–16/);
      assert.equal(f.context.accountNameEditing.value, "first");
    }
    assert.equal(
      f.calls.some((c) => c.op === "rename_account"),
      false,
    );
    assert.equal(
      f.context.snapshot.value.database.accounts[0].username,
      "Original",
    );
  } finally {
    f.dispose();
  }
});

test("failed rename stays visible through blur, reopening and retry, retains the draft and clears on success or dismiss", async () => {
  const f = await setup();
  try {
    f.handlers.rename_account = () => {
      throw Error("名称被占用 fixture");
    };
    f.context.accountNameDraft.value = "Taken_Name";
    await f.context.saveAccountName();
    assert.equal(f.context.visibleError.value, "名称被占用 fixture");
    assert.equal(f.context.accountNameDraft.value, "Taken_Name");
    f.context.blurAccountName({
      currentTarget: { contains: () => false },
      relatedTarget: null,
    });
    await flush();
    assert.equal(f.context.accountNameEditing.value, "first");
    assert.equal(f.context.accountNameDraft.value, "Taken_Name");
    assert.equal(f.context.visibleError.value, "名称被占用 fixture");
    f.context.closeUtility();
    await flush();
    f.context.utility.value = "accounts";
    await f.context.editAccountName(account);
    assert.equal(f.context.visibleError.value, "名称被占用 fixture");
    const pending = deferred();
    f.handlers.rename_account = () => pending.promise;
    f.context.accountNameDraft.value = "New_Name";
    const retry = f.context.saveAccountName();
    await flush();
    assert.equal(
      f.context.visibleError.value,
      "名称被占用 fixture",
      "retry does not blink the old error away",
    );
    assert.equal(
      f.context.saveAccountName(),
      retry,
      "duplicate submit shares the pending write",
    );
    pending.resolve({ ...account, username: "New_Name" });
    await retry;
    assert.equal(f.context.visibleError.value, "");
    await f.context.editAccountName({ ...account, username: "New_Name" });
    f.context.accountNameDraft.value = "x";
    await f.context.saveAccountName();
    f.context.dismissError();
    assert.equal(f.context.visibleError.value, "");
  } finally {
    f.dispose();
  }
});

test("switching identity during rename does not redirect the write or reselect the old account", async () => {
  const f = await setup();
  try {
    const pending = deferred();
    f.handlers.rename_account = ({ id }) => {
      assert.equal(id, "first");
      return pending.promise;
    };
    f.handlers.select_account = () => true;
    f.context.accountNameDraft.value = "New_Name";
    const saving = f.context.saveAccountName();
    await flush();
    await f.context.chooseIdentity(null);
    f.context.closeUtility();
    pending.resolve({ ...account, username: "New_Name" });
    await saving;
    assert.equal(f.context.snapshot.value.database.selected_account, null);
    assert.equal(
      f.context.snapshot.value.database.accounts[0].username,
      "New_Name",
    );
    assert.equal(f.context.accountNameEditing.value, null);
  } finally {
    f.dispose();
  }
});

test("skin and name writes are ordered so neither response can restore stale profile fields", async () => {
  const f = await setup();
  try {
    f.handlers.select_account = () => true;
    await f.context.chooseIdentity("first");
    f.context.utility.value = "accounts";
    await f.context.editAccountName(account);
    const skin = deferred(),
      renamed = deferred();
    f.handlers.resource_manager = () => skin.promise;
    f.handlers.account_skin = () => [1, 2, 3];
    f.handlers.rename_account = () => renamed.promise;
    const changing = f.context.chooseSkin("builtin:cirno");
    f.context.accountNameDraft.value = "New_Name";
    const naming = f.context.saveAccountName();
    await flush();
    assert.equal(
      f.calls.some((c) => c.op === "rename_account"),
      false,
    );
    const updated = {
      ...account,
      skin: { url: "updated-texture", slim: true },
    };
    skin.resolve(updated);
    await changing;
    await flush();
    renamed.resolve({ ...updated, username: "New_Name" });
    await naming;
    assert.equal(f.context.selectedAccount.value.username, "New_Name");
    assert.equal(f.context.selectedAccount.value.skin.url, "updated-texture");
  } finally {
    f.dispose();
  }
});

test("launch waits for a submitted rename so it uses the committed player name", async () => {
  const f = await setup();
  try {
    const pending = deferred();
    f.handlers.rename_account = () => pending.promise;
    f.handlers.launch = () => true;
    f.context.accountNameDraft.value = "New_Name";
    const naming = f.context.saveAccountName();
    const launch = f.context.launch(
      f.context.snapshot.value.database.instances[0],
    );
    await flush();
    assert.equal(
      f.calls.some((c) => c.op === "launch"),
      false,
    );
    f.state.database.accounts[0].username = "New_Name";
    pending.resolve({ ...account, username: "New_Name" });
    await naming;
    await launch;
    assert.equal(
      f.calls.some((c) => c.op === "launch"),
      true,
    );
  } finally {
    f.dispose();
  }
});

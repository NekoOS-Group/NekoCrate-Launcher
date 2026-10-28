import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { mergeBuiltinAccountSkins } from "../src/skinLibrary.ts";

const builtins = ["steve", "cirno"].map((name) => ({
  id: `builtin:${name}`,
  name,
  slim: name === "cirno",
  texture: `bundled:${name}`,
  builtin: true,
  sha256: JSON.parse(
    readFileSync(
      new URL(`../src/assets/${name}.provenance.json`, import.meta.url),
    ),
  ).asset_sha256,
}));
const account = {
  id: "owner",
  username: "CirnoNeko",
  skin: {
    url: `http://textures.minecraft.net/texture/${builtins[1].sha256}`,
    slim: true,
  },
};

test("the owner's skin appears once with its bundled name and in-use marker", () => {
  const choices = mergeBuiltinAccountSkins(builtins, [account]);
  assert.deepEqual(
    choices.map((skin) => skin.name),
    ["steve", "cirno"],
  );
  assert.deepEqual(choices[1].users, [{ id: "owner", name: "CirnoNeko" }]);
  assert.equal(choices[1].texture, "bundled:cirno");
  assert.equal(
    choices[1].account,
    undefined,
    "preview/export must keep using the offline asset",
  );
  assert.equal(choices[1].slim, true);
  assert.deepEqual(mergeBuiltinAccountSkins(builtins, [])[1].users, []);
  assert.equal(
    builtins[1].users,
    undefined,
    "ownership does not mutate the bundled definitions",
  );
});

test("HTTP/HTTPS and multiple owners share one bundled card", () => {
  const second = {
    ...account,
    id: "second",
    username: "Second",
    skin: { ...account.skin, url: account.skin.url.replace("http:", "https:") },
  };
  const choices = mergeBuiltinAccountSkins(builtins, [
    account,
    second,
    { id: "default", username: "Default", skin: null },
  ]);
  assert.equal(choices.length, 2);
  assert.deepEqual(
    choices[1].users.map((user) => user.id),
    ["owner", "second"],
  );
  assert.equal(choices[0].users[0].id, "default");
});

test("changing texture or model retains the separate account skin", () => {
  for (const skin of [
    { ...account.skin, slim: false },
    {
      ...account.skin,
      url: `https://textures.minecraft.net/texture/${"a".repeat(64)}`,
    },
    {
      ...account.skin,
      url: account.skin.url.replace("textures.minecraft.net", "example.com"),
    },
  ]) {
    const choices = mergeBuiltinAccountSkins(builtins, [{ ...account, skin }]);
    assert.equal(choices.length, 3);
    assert.deepEqual(choices[1].users, []);
    assert.equal(choices[2].account, "owner");
    assert.equal(choices[2].textureKey, skin.url);
  }
});

test("uploaded local skins keep account markers on the imported card after server re-encoding", () => {
  const saved = {
    id: "b".repeat(64),
    name: "Local",
    slim: true,
    texture_url: `https://textures.minecraft.net/texture/${"c".repeat(64)}`,
  };
  const player = { ...account, skin: { url: saved.texture_url, slim: true } };
  const choices = mergeBuiltinAccountSkins(builtins, [player], [saved]);
  assert.equal(choices.length, 3);
  assert.equal(choices[2].id, saved.id);
  assert.equal(choices[2].users[0].id, player.id);
  assert.equal(choices[2].account, undefined);
  assert.equal(saved.users, undefined);
  const changed = mergeBuiltinAccountSkins(
    builtins,
    [player],
    [{ ...saved, slim: false }],
  );
  assert.equal(
    changed.length,
    4,
    "editing the arm model does not change the online profile",
  );
});

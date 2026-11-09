import assert from "node:assert/strict";
import test from "node:test";
import {
  homeGroups,
  homeSizes,
  defaultHomeLayout,
  arrangeHomeLayout,
  normalizeHomeLayout,
  expandHomeLayout,
  readHomeLayout,
  readHomeOrientations,
  snapHomeRect,
  overlaps,
  newsDockTarget,
  resizeWidgetRect,
  inferHomeAttachments,
  readHomeAttachments,
} from "../src/homeLayout.ts";

function clear(rects, area) {
  for (const [index, id] of homeGroups.entries()) {
    const rect = rects[id];
    assert.ok(rect.x >= 12 && rect.y >= 12, `${id} leaves the top/left edge`);
    assert.ok(
      rect.x + rect.width <= area.width - 12 + 0.01,
      `${id} leaves the right edge`,
    );
    assert.ok(
      rect.y + rect.height <= area.height - 12 + 0.01,
      `${id} leaves the bottom edge`,
    );
    for (const other of homeGroups.slice(index + 1))
      assert.ok(!overlaps(rect, rects[other]), `${id} overlaps ${other}`);
  }
}
test("default and persisted layouts fit small/large windows, including a full instance dock", () => {
  for (const area of [
    { width: 760, height: 500 },
    { width: 900, height: 600 },
    { width: 1440, height: 900 },
  ]) {
    for (const slots of [0, 1, 4, 8]) {
      const sizes = homeSizes(area, slots);
      const result = arrangeHomeLayout(defaultHomeLayout(area, sizes), area);
      clear(result, area);
      const saved = normalizeHomeLayout(result, area);
      const parsed = readHomeLayout(
        JSON.stringify({ version: 1, positions: saved }),
      );
      assert.deepEqual(parsed, saved);
      const restored = arrangeHomeLayout(
        expandHomeLayout(parsed, area, sizes),
        area,
      );
      for (const id of homeGroups)
        assert.ok(
          Math.abs(restored[id].x - result[id].x) < 0.01 &&
            Math.abs(restored[id].y - result[id].y) < 0.01,
        );
      const small = { width: 760, height: 500 };
      clear(
        arrangeHomeLayout(
          expandHomeLayout(saved, small, homeSizes(small, 8)),
          small,
        ),
        small,
      );
    }
  }
});
test("each dock can rotate independently while remaining in bounds on small and large screens", () => {
  for (const area of [
    { width: 760, height: 500 },
    { width: 1440, height: 900 },
  ]) {
    for (const tools of ["horizontal", "vertical"])
      for (const instances of ["horizontal", "vertical", "wheel"])
        for (const slots of [0, 1, 8, 9]) {
          const directions = { tools, instances };
          const sizes = homeSizes(area, slots, directions);
          assert.equal(sizes.tools.width, tools === "vertical" ? 64 : 144);
          assert.equal(sizes.tools.height, tools === "vertical" ? 144 : 64);
          if (instances === "vertical") assert.equal(sizes.instances.width, 64);
          const layout = arrangeHomeLayout(
            defaultHomeLayout(area, sizes),
            area,
          );
          clear(layout, area);
          const encoded = JSON.stringify({
            version: 1,
            positions: normalizeHomeLayout(layout, area),
            orientations: directions,
          });
          assert.deepEqual(readHomeOrientations(encoded), directions);
          const restored = arrangeHomeLayout(
            expandHomeLayout(
              readHomeLayout(encoded),
              area,
              homeSizes(area, slots, readHomeOrientations(encoded)),
            ),
            area,
          );
          clear(restored, area);
          for (const id of homeGroups) {
            assert.ok(Math.abs(restored[id].x - layout[id].x) < 0.01);
            assert.ok(Math.abs(restored[id].y - layout[id].y) < 0.01);
          }
        }
  }
  assert.deepEqual(
    readHomeOrientations(
      '{"version":1,"orientations":{"tools":"vertical","instances":"invalid"}}',
    ),
    { tools: "vertical", instances: "horizontal" },
  );
  for (const value of [
    null,
    "broken",
    '{"version":1}',
    '{"version":2,"orientations":{"tools":"vertical"}}',
  ])
    assert.deepEqual(readHomeOrientations(value), {
      tools: "horizontal",
      instances: "horizontal",
    });
});
test("dragged controls take priority and neighbours find nonoverlapping positions", () => {
  const area = { width: 760, height: 500 };
  const defaults = arrangeHomeLayout(
    defaultHomeLayout(area, homeSizes(area, 8)),
    area,
  );
  const toolbar = { x: 568, y: 12, width: 180, height: 36 };
  for (const id of homeGroups) {
    for (let x = -60; x < 800; x += 37)
      for (let y = -40; y < 530; y += 29) {
        const layout = arrangeHomeLayout(
          { ...defaults, [id]: { ...defaults[id], x, y } },
          area,
          id,
          [toolbar],
        );
        clear(layout, area);
        for (const rect of Object.values(layout))
          assert.ok(!overlaps(rect, toolbar));
      }
  }
});
test("legacy combined account/tools layouts split in place without resetting other controls", () => {
  const area = { width: 900, height: 600 },
    sizes = homeSizes(area, 4);
  const positions = {
    account: { x: 0.5, y: 0.25 },
    instances: { x: 0.8, y: 1 },
    themes: { x: 0, y: 1 },
    launch: { x: 1, y: 0.8 },
  };
  const parsed = readHomeLayout(JSON.stringify({ version: 1, positions }));
  assert.deepEqual(parsed, positions);
  const rects = expandHomeLayout(parsed, area, sizes);
  assert.equal(rects.account.x, 12 + (900 - 380 - 24) * 0.5);
  assert.equal(rects.tools.x, rects.account.x + rects.account.width + 12);
  assert.equal(rects.tools.y, rects.account.y);
  const saved = normalizeHomeLayout(rects, area);
  const reloaded = expandHomeLayout(
    readHomeLayout(JSON.stringify({ version: 1, positions: saved })),
    area,
    sizes,
  );
  for (const id of homeGroups) {
    assert.ok(Math.abs(reloaded[id].x - rects[id].x) < 0.01);
    assert.ok(Math.abs(reloaded[id].y - rects[id].y) < 0.01);
    assert.equal(reloaded[id].width, rects[id].width);
    assert.equal(reloaded[id].height, rects[id].height);
  }
  assert.deepEqual(saved.instances, positions.instances);
  assert.deepEqual(saved.themes, positions.themes);
  assert.deepEqual(saved.launch, positions.launch);
  assert.equal(
    readHomeLayout(
      JSON.stringify({ version: 1, positions: { ...positions, tools: {} } }),
    ),
    null,
  );
});
test("alignment snaps to edges and other groups without dragging distant controls", () => {
  const area = { width: 900, height: 600 };
  const rect = { x: 15, y: 510, width: 206, height: 56 };
  const edge = snapHomeRect(rect, [], area);
  assert.equal(edge.rect.x, 12);
  assert.equal(edge.guides.x, 12);
  const aligned = snapHomeRect(
    { ...rect, x: 303, y: 203 },
    [{ x: 300, y: 200, width: 380, height: 64 }],
    area,
  );
  assert.equal(aligned.rect.x, 300);
  assert.equal(aligned.rect.y + rect.height / 2, 232);
  assert.equal(
    snapHomeRect({ ...rect, x: 90, y: 90 }, [], area).guides.x,
    undefined,
  );
});
test("corrupt, old and out-of-range layouts fall back without accepting partial settings", () => {
  for (const value of [
    null,
    "broken",
    "{}",
    '{"version":1,"positions":{}}',
    '{"version":2,"positions":{}}',
  ])
    assert.equal(readHomeLayout(value), null);
  const positions = Object.fromEntries(
    homeGroups.map((id) => [id, { x: 0, y: 1 }]),
  );
  positions.account.x = 1.01;
  assert.equal(readHomeLayout(JSON.stringify({ version: 1, positions })), null);
});

test("controls stay clear of each other; all widgets stay within the window", () => {
  for (const area of [
    { width: 900, height: 600 },
    { width: 1440, height: 900 },
  ]) {
    for (const tools of ["horizontal", "vertical"])
      for (const instances of ["horizontal", "vertical", "wheel"]) {
        const sizes = homeSizes(area, 8, { tools, instances });
        const layout = arrangeHomeLayout(
          defaultHomeLayout(area, sizes, ["quote", "news"]),
          area,
        );
        const entries = Object.entries(layout);
        for (let i = 0; i < entries.length; i++) {
          const [id, rect] = entries[i];
          assert.ok(
            rect.x >= 12 &&
              rect.y >= 12 &&
              rect.x + rect.width <= area.width - 12 &&
              rect.y + rect.height <= area.height - 12,
          );
          for (const [otherId, other] of entries.slice(i + 1))
            if (homeGroups.includes(id) && homeGroups.includes(otherId))
              assert.ok(!overlaps(rect, other));
        }
        const saved = normalizeHomeLayout(layout, area);
        const decoded = readHomeLayout(
          JSON.stringify({
            version: 1,
            positions: saved,
            widgets: ["quote", "news"],
          }),
        );
        assert.deepEqual(decoded, saved);
        const restored = expandHomeLayout(decoded, area, sizes, [
          "quote",
          "news",
        ]);
        for (const [id, rect] of Object.entries(layout)) {
          assert.ok(Math.abs(restored[id].x - rect.x) < 0.001);
          assert.ok(Math.abs(restored[id].y - rect.y) < 0.001);
        }
      }
  }
});

test("text overlays overlap controls in either drag direction and survive save, resize and text sizing", () => {
  const area = { width: 900, height: 600 };
  const sizes = homeSizes(area, 4);
  const base = arrangeHomeLayout(
    defaultHomeLayout(area, sizes, ["quote", "news"]),
    area,
  );
  const requested = {
    ...base,
    quote: { ...base.quote, x: base.account.x, y: base.account.y },
  };
  const layout = arrangeHomeLayout(requested, area, "quote");
  assert.deepEqual(layout, requested);
  assert.ok(overlaps(layout.quote, layout.account));
  const saved = normalizeHomeLayout(layout, area);
  for (const scale of [0.6, 1, 1.6]) {
    for (const viewport of [area, { width: 760, height: 500 }]) {
      const expanded = expandHomeLayout(
        saved,
        viewport,
        homeSizes(viewport, 4, undefined, 3, scale),
        ["quote", "news"],
      );
      const restored = arrangeHomeLayout(expanded, viewport);
      assert.deepEqual(restored.quote, expanded.quote);
      const { quote, ...controls } = expanded;
      assert.deepEqual(
        Object.fromEntries(
          Object.entries(restored).filter(([id]) => id !== "quote"),
        ),
        arrangeHomeLayout(controls, viewport),
      );
    }
  }
  // Moving a control onto a quote leaves the quote in place too.
  const controlMoved = {
    ...base,
    account: { ...base.account, x: base.quote.x, y: base.quote.y },
  };
  const result = arrangeHomeLayout(controlMoved, area, "account");
  assert.deepEqual(result.quote, base.quote);
  assert.deepEqual(result.account, controlMoved.account);
  assert.ok(overlaps(result.account, result.quote));
  // Editor buttons remain accessible even when text overlaps other layout boxes.
  const toolbar = { x: 668, y: 12, width: 210, height: 36 };
  const nearToolbar = arrangeHomeLayout(
    { ...base, quote: { ...base.quote, x: 650, y: 12 } },
    area,
    "quote",
    [toolbar],
  );
  assert.ok(!overlaps(nearToolbar.quote, toolbar));
});

test("news docks within the visible edge snap zone, releases inward, and does not displace controls", () => {
  const area = { width: 900, height: 600 };
  const base = arrangeHomeLayout(
    defaultHomeLayout(area, homeSizes(area, 4), ["news"]),
    area,
  );
  const news = { ...base.news, y: 20 };
  assert.equal(newsDockTarget({ ...news, x: 12 }, area), "left");
  assert.equal(newsDockTarget({ ...news, x: 22 }, area), "left");
  assert.equal(newsDockTarget({ ...news, x: 23 }, area), null);
  assert.equal(newsDockTarget({ ...news, x: 578 }, area), "right");
  assert.equal(newsDockTarget({ ...news, x: 577 }, area), null);
  assert.equal(newsDockTarget({ ...news, x: -9 }, area), "left");
  assert.equal(newsDockTarget({ ...news, x: 609 }, area), "right");
  assert.equal(newsDockTarget({ ...news, x: 15 }, area, "left"), "left");
  assert.equal(newsDockTarget({ ...news, x: 25 }, area, "left"), "left");
  assert.equal(newsDockTarget({ ...news, x: 36 }, area, "left"), null);
  assert.equal(newsDockTarget({ ...news, x: 580 }, area, "right"), "right");
  assert.equal(newsDockTarget({ ...news, x: 564 }, area, "right"), null);
  const { news: unused, ...controls } = base;
  for (const viewport of [area, { width: 760, height: 500 }]) {
    for (const side of ["left", "right"]) {
      const result = arrangeHomeLayout(
        { ...base, news },
        viewport,
        "news",
        [],
        side,
      );
      assert.equal(
        result.news.x,
        side === "left" ? 12 : viewport.width - result.news.width - 12,
      );
      assert.deepEqual(
        Object.fromEntries(
          Object.entries(result).filter(([id]) => id !== "news"),
        ),
        arrangeHomeLayout(controls, viewport),
      );
    }
  }
});

test("Trends height keeps the opposite edge fixed and clamps to available space", () => {
  const area = { width: 900, height: 600 };
  const rect = { x: 588, y: 100, width: 300, height: 244 };
  assert.deepEqual(resizeWidgetRect(rect, "bottom", 100, area), {
    ...rect,
    height: 344,
  });
  assert.deepEqual(resizeWidgetRect(rect, "top", -70, area), {
    ...rect,
    y: 30,
    height: 314,
  });
  assert.equal(resizeWidgetRect(rect, "top", -999, area).y, 12);
  assert.equal(resizeWidgetRect(rect, "bottom", -999, area).height, 120);
  const full = resizeWidgetRect(rect, "bottom", 999, area);
  assert.equal(full.y + full.height, 588);
  assert.equal(
    homeSizes({ width: 760, height: 500 }, 4, undefined, 3, 1, 700).news.height,
    476,
  );
  assert.equal(
    homeSizes({ width: 1600, height: 1000 }, 4, undefined, 3, 1, 700).news
      .height,
    700,
  );
});

test("saved adjacent controls retain gaps through resize, reload, orientation and content size changes", () => {
  const area = { width: 900, height: 600 };
  const base = arrangeHomeLayout(
    defaultHomeLayout(area, homeSizes(area, 4)),
    area,
  );
  const positions = normalizeHomeLayout(base, area);
  const attachments = inferHomeAttachments(base);
  assert.equal(attachments.tools.side, "right");
  assert.equal(attachments.instances.side, "left");
  const stored = JSON.stringify({ version: 1, positions, attachments });
  assert.deepEqual(readHomeAttachments(stored), attachments);
  for (const viewport of [
    area,
    { width: 1440, height: 900 },
    { width: 760, height: 500 },
  ]) {
    for (const toolCount of [1, 3, 4]) {
      const layout = arrangeHomeLayout(
        expandHomeLayout(
          readHomeLayout(stored),
          viewport,
          homeSizes(viewport, 5, undefined, toolCount),
          [],
          readHomeAttachments(stored),
        ),
        viewport,
      );
      clear(layout, viewport);
      assert.ok(
        Math.abs(
          layout.tools.x - layout.account.x - layout.account.width - 12,
        ) < 0.01,
      );
      assert.ok(
        Math.abs(
          layout.launch.x - layout.instances.x - layout.instances.width - 12,
        ) < 0.01,
      );
      assert.ok(
        Math.abs(
          layout.launch.y +
            layout.launch.height / 2 -
            layout.instances.y -
            layout.instances.height / 2,
        ) < 0.01,
      );
    }
  }
  const vertical = {
    ...base,
    tools: {
      ...base.tools,
      width: 64,
      height: 144,
      x: base.account.x + 80,
      y: base.account.y + base.account.height + 12,
    },
  };
  const links = inferHomeAttachments(vertical);
  assert.equal(links.tools.side, "bottom");
  const large = { width: 1440, height: 900 };
  const restored = expandHomeLayout(
    normalizeHomeLayout(vertical, area),
    large,
    homeSizes(large, 4, { tools: "vertical", instances: "horizontal" }),
    [],
    links,
  );
  assert.equal(
    restored.tools.y - restored.account.y - restored.account.height,
    12,
  );
  assert.equal(
    inferHomeAttachments({ ...base, tools: { ...base.tools, x: 500, y: 300 } })
      .tools,
    undefined,
  );
  assert.deepEqual(
    readHomeAttachments(
      '{"version":1,"attachments":{"tools":{"side":"right","gap":-100,"offset":0}}}',
    ),
    {},
  );
  assert.equal(readHomeAttachments("broken"), null);
});

test("hidden controls reserve no collision space and do not anchor visible controls on resize", () => {
  const area = { width: 760, height: 500 },
    sizes = homeSizes(area, 8);
  const defaults = defaultHomeLayout(area, sizes, ["servers"]);
  const requested = {
    ...defaults,
    themes: { ...defaults.themes, x: 100, y: 180 },
    servers: { ...defaults.servers, x: 100, y: 180 },
  };
  const hidden = arrangeHomeLayout(requested, area, undefined, [], null, [
    "themes",
  ]);
  assert.equal(hidden.servers.x, 100);
  assert.equal(hidden.servers.y, 180);
  assert.deepEqual(
    hidden.themes,
    requested.themes,
    "keep the old location for re-adding",
  );
  const saved = normalizeHomeLayout(defaults, area);
  saved.launch = { x: 0, y: 1 };
  const link = { instances: { side: "left", gap: 12, offset: 0 } };
  const unlinked = expandHomeLayout(saved, area, sizes);
  const linked = expandHomeLayout(saved, area, sizes, [], link);
  const hiddenDock = expandHomeLayout(saved, area, sizes, [], link, [
    "instances",
  ]);
  assert.notEqual(linked.launch.x, unlinked.launch.x);
  assert.deepEqual(hiddenDock.launch, unlinked.launch);
  assert.equal(
    inferHomeAttachments(hiddenDock, ["instances"]).instances,
    undefined,
  );
});

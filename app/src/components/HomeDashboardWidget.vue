<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  Check,
  GripVertical,
  Map,
  Plus,
  Server,
  Signal,
  X,
} from "lucide-vue-next";
import InstanceIcon from "./InstanceIcon.vue";
import { locale, t } from "../i18n";
import {
  dashboard,
  saveDashboard,
  shortcutKey,
  resolveShortcut,
  normalizeServerAddress,
  loadLocalWidgets,
  localWidgetData,
  serverStatuses,
  loadServerStatus,
  pixelTimePath,
  type VersionShortcut,
  type LaunchDestination,
  type ServerShortcut,
  type RecentWorld,
  type Todo,
} from "../homeDashboard";
import { resourceTransferKey } from "../useResourceTransfer";
import { useWidgetSorting } from "../useWidgetSorting";
import type { WidgetRpc } from "../widgetClient";
import type { Instance } from "../types";

const props = defineProps<{
  kind: "servers" | "worlds" | "todos" | "clock";
  editing: boolean;
  instances: Instance[];
  selected?: Instance;
  running: string[];
  busy: boolean;
  rpc: WidgetRpc;
  clockScale?: number;
  clockAngle?: number;
  clockDate?: boolean;
  align?: "left" | "right";
}>();
const emit = defineEmits<{
  launch: [target: VersionShortcut, destination?: LaunchDestination];
  error: [error: unknown];
}>();
const host = ref<HTMLElement>();
const transfer = inject(resourceTransferKey, undefined);
const visible = ref(true),
  localError = ref(false),
  addingServer = ref(false);
const serverName = ref(""),
  serverAddress = ref(""),
  serverError = ref("");
const todoText = ref("");
const serverMenu = ref<string>();
function decode(value: string): VersionShortcut | undefined {
  try {
    const [instance, version] = JSON.parse(value);
    const result = { instance, version };
    return resolveShortcut(props.instances, result) ? result : undefined;
  } catch {
    return undefined;
  }
}
function persist(change: Partial<typeof dashboard.value>) {
  try {
    saveDashboard({ ...dashboard.value, ...change });
  } catch (error) {
    emit("error", error);
  }
}
const todoItems = computed(() => dashboard.value.todos);
const todoSort = useWidgetSorting(
  todoItems,
  (item) => item.id,
  (todos) => persist({ todos }),
);
const todoRows = computed(() => todoSort.rows.value as Todo[]);
const exampleTodos = ["给小屋补一扇窗", "备份生存世界", "整理一下 Mods"];
function addTodo() {
  const text = todoText.value.trim();
  if (!text) return;
  if (dashboard.value.todos.length >= 100) {
    emit("error", t("最多添加 100 条待办"));
    return;
  }
  persist({
    todos: [
      ...dashboard.value.todos,
      { id: crypto.randomUUID(), text, done: false },
    ],
  });
  todoText.value = "";
}
function updateTodo(id: string, patch: Partial<Todo>) {
  persist({
    todos: dashboard.value.todos.map((item) =>
      item.id === id ? { ...item, ...patch } : item,
    ),
  });
}
function removeTodo(id: string) {
  persist({ todos: dashboard.value.todos.filter((item) => item.id !== id) });
}
function renameTodo(event: Event, item: Todo) {
  const input = event.target as HTMLInputElement,
    text = input.value.trim();
  if (text) updateTodo(item.id, { text });
  else input.value = item.text;
}
function savedServerKey(item: Omit<ServerShortcut, "id">) {
  return `local:${JSON.stringify([item.instance, item.address])}`;
}
const servers = computed<ServerShortcut[]>(() => {
  const custom = dashboard.value.servers,
    seen = new Set(custom.map((item) => item.address));
  return [
    ...custom,
    ...localWidgetData.value.servers.flatMap((item) => {
      const id = savedServerKey(item);
      if (seen.has(item.address) || dashboard.value.hiddenServers.includes(id))
        return [];
      seen.add(item.address);
      return [{ ...item, id }];
    }),
  ];
});
const serverRows = computed(() =>
  props.editing && !servers.value.length
    ? [
        {
          id: "example",
          name: t("我的服务器"),
          address: "localhost:25565",
          instance: "",
          version: "",
        },
      ]
    : servers.value,
);
function serverTarget(_server: ServerShortcut) {
  const selected = props.selected;
  if (selected?.selected_version)
    return shortcutKey({
      instance: selected.id,
      version: selected.selected_version,
    });
  const fallback = props.instances.find((instance) => instance.versions.length);
  return fallback
    ? shortcutKey({ instance: fallback.id, version: fallback.versions[0]!.id })
    : "";
}
function connect(server: ServerShortcut) {
  const target = decode(serverTarget(server));
  if (target) emit("launch", target, { server: server.address });
}
function addServer() {
  const address = normalizeServerAddress(serverAddress.value);
  if (!address) {
    serverError.value = "请输入有效的服务器地址";
    return;
  }
  if (servers.value.some((item) => item.address === address)) {
    serverError.value = "已添加此服务器";
    return;
  }
  if (dashboard.value.servers.length >= 32) {
    serverError.value = "最多添加 32 个服务器";
    return;
  }
  const target = props.selected?.selected_version
    ? { instance: props.selected.id, version: props.selected.selected_version }
    : { instance: "", version: "" };
  persist({
    servers: [
      ...dashboard.value.servers,
      {
        ...target,
        id: crypto.randomUUID(),
        name: serverName.value.trim() || address,
        address,
      },
    ],
  });
  serverName.value = serverAddress.value = serverError.value = "";
  addingServer.value = false;
  void refreshServers();
}
function removeServer(server: ServerShortcut) {
  if (server.id.startsWith("local:"))
    persist({ hiddenServers: [...dashboard.value.hiddenServers, server.id] });
  else
    persist({
      servers: dashboard.value.servers.filter((item) => item.id !== server.id),
    });
}
function dropAddress(event: DragEvent) {
  if (props.editing || event.dataTransfer?.files.length) return;
  const address = normalizeServerAddress(
    event.dataTransfer?.getData("text/plain") || "",
  );
  if (!address) return;
  event.preventDefault();
  event.stopPropagation();
  addingServer.value = true;
  serverAddress.value = address;
}
const worlds = computed(() => localWidgetData.value.worlds);
function ago(value: number) {
  const minutes = Math.floor(Math.max(0, Date.now() - value) / 60_000);
  const formatter = new Intl.RelativeTimeFormat(locale.value, {
    numeric: "auto",
  });
  return minutes < 60
    ? formatter.format(-minutes, "minute")
    : minutes < 1440
      ? formatter.format(-Math.floor(minutes / 60), "hour")
      : formatter.format(-Math.floor(minutes / 1440), "day");
}
function openWorld(world: RecentWorld) {
  if (props.editing || props.busy || props.running.includes(world.instance))
    return;
  emit(
    "launch",
    { instance: world.instance, version: world.version },
    { world: world.file },
  );
}
function dragWorld(event: PointerEvent, world: RecentWorld) {
  if (!props.editing && !props.busy && !props.running.includes(world.instance))
    transfer?.(event, { ...world, kind: "saves", removable: true });
}
let queryingServers = false,
  refreshing = false,
  refreshTimer: ReturnType<typeof setInterval> | undefined,
  observer: IntersectionObserver | undefined,
  disposed = false;
async function refreshServers() {
  if (
    queryingServers ||
    props.kind !== "servers" ||
    props.editing ||
    document.hidden ||
    !visible.value
  )
    return;
  queryingServers = true;
  const addresses = [...new Set(servers.value.map((item) => item.address))];
  // At most three sockets, cached for one minute; never refresh on every render.
  let cursor = 0;
  await Promise.all(
    Array.from({ length: Math.min(3, addresses.length) }, async () => {
      while (
        !disposed &&
        !document.hidden &&
        visible.value &&
        cursor < addresses.length
      )
        await loadServerStatus(props.rpc, addresses[cursor++]!);
    }),
  );
  queryingServers = false;
}
async function refresh(fresh = false) {
  if (
    !["servers", "worlds"].includes(props.kind) ||
    refreshing ||
    disposed ||
    document.hidden ||
    !visible.value ||
    props.editing
  )
    return;
  refreshing = true;
  try {
    await loadLocalWidgets(props.rpc, fresh);
    localError.value = false;
  } catch {
    localError.value = true;
  }
  if (!disposed) await refreshServers();
  refreshing = false;
}
const time = ref(new Date());
let clockTimer: ReturnType<typeof setTimeout> | undefined;
function updateTime() {
  clearTimeout(clockTimer);
  if (props.kind !== "clock" || document.hidden || !visible.value || disposed)
    return;
  time.value = new Date();
  clockTimer = setTimeout(updateTime, 60_000 - (Date.now() % 60_000) + 20);
}
const timeText = computed(
  () =>
    `${String(time.value.getHours()).padStart(2, "0")}:${String(time.value.getMinutes()).padStart(2, "0")}`,
);
const clockPath = computed(() => pixelTimePath(timeText.value));
const dateText = computed(() =>
  new Intl.DateTimeFormat(locale.value, {
    month: "long",
    day: "numeric",
    weekday: "short",
  }).format(time.value),
);
function resume() {
  updateTime();
  if (!document.hidden) void refresh();
}
watch(
  () => props.editing,
  (value) => {
    if (!value) resume();
  },
);
watch(
  () =>
    props.instances
      .map((i) => `${i.id}:${i.versions.map((v) => v.id).join(",")}`)
      .join("|"),
  () => void refresh(true),
);
watch(
  () => props.running.join("|"),
  () => void refresh(true),
);
onMounted(() => {
  observer = new IntersectionObserver((entries) => {
    visible.value = entries[0]?.isIntersecting ?? false;
    resume();
  });
  if (host.value) observer.observe(host.value);
  resume();
  if (["servers", "worlds"].includes(props.kind))
    refreshTimer = setInterval(() => void refresh(), 60_000);
  document.addEventListener("visibilitychange", resume);
});
onBeforeUnmount(() => {
  disposed = true;
  observer?.disconnect();
  clearInterval(refreshTimer);
  clearTimeout(clockTimer);
  document.removeEventListener("visibilitychange", resume);
});
</script>

<template>
  <section
    ref="host"
    class="dashboard-widget"
    :class="`dashboard-${kind}`"
    :data-align="align"
    data-drop-kind="blocked"
    @dragstart.prevent
  >
    <template v-if="kind === 'clock'">
      <div
        class="pixel-clock"
        :style="{ transform: `rotate(${clockAngle || 0}deg)` }"
        :aria-label="`${timeText} ${dateText}`"
        role="timer"
      >
        <svg viewBox="0 0 19 5" aria-hidden="true" shape-rendering="crispEdges">
          <path :d="clockPath" />
        </svg>
        <span v-if="clockDate !== false">{{ dateText }}</span>
      </div>
    </template>
    <template v-else-if="kind === 'todos'">
      <div class="widget-scroll">
        <TransitionGroup
          name="widget-row"
          tag="div"
          class="todo-list"
          data-widget-list
        >
          <div
            v-for="item in todoRows"
            :key="item.id"
            :data-widget-row="item.id"
            class="todo-row"
            :class="{ completed: item.done }"
          >
            <button
              class="todo-check"
              role="checkbox"
              :aria-checked="item.done"
              :aria-label="item.text"
              @click="updateTodo(item.id, { done: !item.done })"
            >
              <Check v-if="item.done" :size="12" />
            </button>
            <input
              :value="item.text"
              maxlength="240"
              :aria-label="t('编辑待办')"
              @change="renameTodo($event, item)"
              @keydown.enter="($event.target as HTMLInputElement).blur()"
            />
            <button
              class="widget-action"
              :aria-label="t('拖动排序')"
              @pointerdown="todoSort.start($event, item.id)"
              @keydown="todoSort.nudge($event, item.id)"
            >
              <GripVertical :size="12" />
            </button>
            <button
              class="widget-action"
              :aria-label="t('删除待办')"
              @click="removeTodo(item.id)"
            >
              <X :size="12" />
            </button>
          </div>
          <div
            v-if="editing && !todoRows.length"
            v-for="(label, index) in exampleTodos"
            :key="label"
            class="todo-row widget-example"
            :class="{ completed: index === 1 }"
          >
            <span class="todo-check"
              ><Check v-if="index === 1" :size="12" /></span
            ><span>{{ t(label) }}</span>
          </div>
        </TransitionGroup>
      </div>
      <form class="widget-inline-add" @submit.prevent="addTodo">
        <Plus :size="14" /><input
          v-model="todoText"
          maxlength="240"
          :placeholder="t('记下一件事…')"
          :aria-label="t('添加待办')"
        /><button
          v-if="todoText.trim()"
          class="widget-small-button"
          :aria-label="t('添加待办')"
        >
          <Check :size="14" />
        </button>
      </form>
    </template>
    <template v-else-if="kind === 'worlds'">
      <div class="widget-scroll">
        <TransitionGroup name="widget-row" tag="div">
          <div
            v-for="world in worlds"
            :key="world.key"
            class="world-row"
            @pointerdown="dragWorld($event, world)"
          >
            <button
              class="world-main"
              :title="t('启动并进入 {name}', { name: world.name })"
              :disabled="editing || busy || running.includes(world.instance)"
              @click="openWorld(world)"
            >
              <img v-if="world.icon" :src="world.icon" alt="" /><Map
                v-else
                :size="32"
              /><span
                ><strong>{{ world.name }}</strong
                ><small
                  >{{ world.game_version || world.version }} ·
                  {{ ago(world.last_played) }}</small
                ></span
              >
            </button>
          </div>
          <div
            v-if="editing && !worlds.length"
            v-for="(name, index) in ['山间小屋', '生存世界']"
            :key="name"
            class="world-row widget-example"
          >
            <InstanceIcon :icon="[4, 0][index]!" /><span
              ><strong>{{ t(name) }}</strong
              ><small>1.21.1 · {{ t("最近游玩") }}</small></span
            >
          </div>
        </TransitionGroup>
        <span v-if="!worlds.length && !editing" class="widget-empty-line">{{
          t(localError ? "暂时无法读取存档" : "还没有本地存档")
        }}</span>
      </div>
    </template>
    <template v-else-if="kind === 'servers'">
      <div class="widget-scroll" @dragover.prevent @drop="dropAddress">
        <TransitionGroup name="widget-row" tag="div">
          <div
            v-for="server in serverRows"
            :key="server.id"
            class="server-row"
            @contextmenu.prevent="serverMenu = server.id"
            @keydown.esc="serverMenu = undefined"
            @focusout="
              !($event.currentTarget as HTMLElement).contains(
                $event.relatedTarget as Node,
              ) && (serverMenu = undefined)
            "
          >
            <button
              class="server-main"
              :disabled="
                busy ||
                !serverTarget(server) ||
                running.includes(decode(serverTarget(server))?.instance || '')
              "
              :title="
                serverStatuses[server.address]?.data?.motd ||
                serverStatuses[server.address]?.error ||
                server.address
              "
              @click="connect(server)"
            >
              <img
                v-if="serverStatuses[server.address]?.data?.icon || server.icon"
                :src="
                  serverStatuses[server.address]?.data?.icon || server.icon!
                "
                alt=""
              /><Server v-else :size="24" />
              <span
                ><strong>{{ server.name }}</strong
                ><small>{{ server.address }}</small></span
              >
              <span
                class="server-status"
                :class="{ online: serverStatuses[server.address]?.data }"
                ><template v-if="serverStatuses[server.address]?.data"
                  ><span
                    >{{ serverStatuses[server.address]!.data!.online
                    }}<i
                      >/{{ serverStatuses[server.address]!.data!.max }}</i
                    ></span
                  ><small
                    ><Signal :size="11" />{{
                      serverStatuses[server.address]!.data!.latency
                    }}
                    ms</small
                  ></template
                ><small v-else>{{
                  t(
                    serverStatuses[server.address]?.error
                      ? "暂不可达"
                      : editing
                        ? "在线状态"
                        : "查询中",
                  )
                }}</small></span
              >
            </button>
            <div
              v-if="serverMenu === server.id"
              class="server-context glass-surface"
              @pointerdown.stop
              @mouseleave="serverMenu = undefined"
            >
              <button
                class="widget-text-button"
                @click="
                  removeServer(server);
                  serverMenu = undefined;
                "
              >
                {{ t("移除服务器") }}
              </button>
            </div>
          </div>
        </TransitionGroup>
        <span v-if="!serverRows.length" class="widget-empty-line">{{
          t("添加或拖入服务器地址")
        }}</span>
      </div>
      <Transition name="widget-row"
        ><form
          v-if="addingServer"
          class="widget-server-form"
          @submit.prevent="addServer"
        >
          <input
            v-model="serverName"
            :placeholder="t('服务器名称')"
            :aria-label="t('服务器名称')"
            maxlength="80"
          /><input
            v-model="serverAddress"
            :placeholder="t('地址，例如 localhost:25565')"
            :aria-label="t('服务器地址')"
            maxlength="255"
            required
            @input="serverError = ''"
          /><small v-if="serverError">{{ t(serverError) }}</small
          ><button
            type="submit"
            class="widget-small-button"
            :aria-label="t('添加服务器')"
          >
            <Check :size="14" />
          </button></form
      ></Transition>
      <div
        class="widget-footer server-footer"
        :class="{ expanded: addingServer }"
      >
        <button
          class="widget-small-button"
          :aria-label="t(addingServer ? '关闭' : '添加服务器')"
          @click="addingServer = !addingServer"
        >
          <X v-if="addingServer" :size="14" /><Plus v-else :size="14" /></button
        ><button
          v-if="dashboard.hiddenServers.length"
          class="widget-text-button"
          @click="persist({ hiddenServers: [] })"
        >
          {{ t("显示游戏内服务器") }}
        </button>
      </div>
    </template>
  </section>
</template>

<style>
.dashboard-widget {
  height: 100%;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  color: var(--ink);
  font-size: 12px;
  user-select: none;
  -webkit-user-select: none;
}
.dashboard-widget button {
  font: inherit;
}
.widget-scroll > div,
.todo-list {
  position: relative;
}
.widget-scroll {
  min-height: 0;
  overflow: auto;
  scrollbar-width: none;
  flex: 1;
  mask-image: linear-gradient(#000 calc(100% - 15px), transparent);
  padding-bottom: 15px;
}
.dashboard-servers .widget-scroll,
.dashboard-todos .widget-scroll {
  flex: 0 1 auto;
}
.dashboard-servers {
  gap: 2px;
}
.dashboard-servers .widget-scroll {
  padding-bottom: 0;
  mask-image: none;
}
.widget-scroll::-webkit-scrollbar {
  display: none;
}
.widget-empty-line {
  display: block;
  color: var(--muted);
  font-size: 11px;
  padding: 12px 2px;
}
.widget-example {
  pointer-events: none;
}
.widget-action,
.widget-small-button,
.widget-text-button {
  display: inline-grid;
  place-items: center;
  border: 0;
  background: transparent;
  padding: 3px;
  color: var(--muted);
  cursor: pointer;
}
.widget-action {
  width: 22px;
  height: 24px;
  flex-shrink: 0;
  opacity: 0;
  transition:
    opacity 120ms,
    color 120ms;
}
:is(.todo-row, .world-row, .server-row):is(:hover, :focus-within)
  .widget-action,
.widget-action:focus-visible {
  opacity: 1;
}
.dashboard-widget button:is(:hover, :focus-visible) {
  color: var(--blue);
  outline: none;
}
.dashboard-widget button:focus-visible {
  filter: drop-shadow(0 0 4px var(--glow));
}
.dashboard-widget button:disabled {
  opacity: 0.5;
  cursor: default;
}
.dashboard-widget strong {
  font-weight: 500;
  display: block;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.dashboard-widget small {
  display: block;
  color: var(--muted);
  font-size: 10px;
  line-height: 1.55;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.dashboard-widget input {
  min-width: 0;
  border: 0;
  border-radius: 0;
  background: transparent;
  padding: 4px 0;
  height: 28px;
  color: var(--ink);
  font-size: 12px;
  box-shadow: none;
}
.dashboard-widget input:focus {
  outline: none;
  box-shadow: none;
}
.dashboard-widget input::placeholder {
  color: var(--muted);
  opacity: 0.65;
}
.widget-inline-add {
  display: flex;
  align-items: center;
  gap: 9px;
  color: var(--muted);
}
.widget-inline-add input {
  flex: 1;
}
.widget-footer {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 22px;
}
.widget-text-button {
  font-size: 10px !important;
}
.todo-row {
  display: flex;
  align-items: center;
  gap: 9px;
  min-height: 33px;
}
.todo-row > input,
.todo-row > span:last-child {
  flex: 1;
}
.todo-row .widget-action {
  margin-left: -6px;
  touch-action: none;
}
.todo-check {
  width: 14px;
  height: 14px;
  flex: 0 0 14px;
  display: grid;
  place-items: center;
  border: 1px solid color-mix(in srgb, var(--blue) 50%, transparent);
  border-radius: 1px;
  background: transparent;
  padding: 0;
  color: var(--blue);
}
.todo-row.completed > input,
.todo-row.completed > span:last-child {
  color: var(--muted);
  text-decoration: line-through;
  opacity: 0.65;
}
.world-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 56px;
}
.world-main,
.server-main {
  min-width: 0;
  flex: 1;
  display: flex;
  gap: 10px;
  align-items: center;
  text-align: left;
  padding: 4px 0;
  border: 0;
  background: transparent;
  color: var(--ink);
  cursor: pointer;
}
.world-main > span,
.world-row > span,
.server-main > span:not(.server-status) {
  flex: 1;
  min-width: 0;
}
.world-main > img,
.world-row > .instance-art {
  width: 38px;
  height: 38px;
  object-fit: cover;
  image-rendering: pixelated;
  flex-shrink: 0;
}
.world-main > svg {
  flex: 0 0 32px;
  color: var(--blue);
}
.server-row {
  padding: 3px 0 5px;
}
.server-main {
  width: 100%;
  gap: 9px;
}
.server-main > img {
  width: 28px;
  height: 28px;
  image-rendering: pixelated;
  flex-shrink: 0;
}
.server-main > svg {
  flex: 0 0 24px;
  color: var(--blue);
}
.server-status {
  text-align: right;
  font-variant-numeric: tabular-nums;
  font-size: 11px;
}
.server-status i {
  font-style: normal;
  opacity: 0.45;
  font-size: 10px;
}
.server-status small {
  display: flex;
  align-items: center;
  gap: 3px;
  justify-content: flex-end;
  font-size: 9px;
}
.server-status.online {
  color: var(--blue);
}
.server-row {
  position: relative;
}
.server-context {
  position: absolute;
  right: 0;
  top: 0;
  padding: 8px;
  z-index: 2;
}
.dashboard-worlds[data-align="right"] .world-row,
.dashboard-worlds[data-align="right"] .world-main,
.dashboard-servers[data-align="right"] .server-main {
  flex-direction: row-reverse;
  text-align: right;
}
.dashboard-servers[data-align="right"] .server-status {
  text-align: left;
}
.dashboard-servers[data-align="right"] .server-status small {
  justify-content: flex-start;
}
.dashboard-servers[data-align="right"] .server-context {
  left: 0;
  right: auto;
}
.dashboard-worlds[data-align="right"] .widget-empty-line,
.dashboard-servers[data-align="right"] .widget-empty-line {
  text-align: right;
}
.server-footer {
  position: relative;
  justify-content: center;
}
.server-footer > .widget-text-button {
  position: absolute;
  left: calc(50% + 18px);
  max-width: calc(50% - 18px);
  font-size: 9px !important;
}
.server-footer > button {
  opacity: 0;
  transition:
    opacity 150ms,
    color 150ms;
}
.dashboard-servers:is(:hover, :focus-within) .server-footer > button,
.server-footer.expanded > button {
  opacity: 1;
}
.widget-server-form {
  position: relative;
  display: flex;
  flex-direction: column;
  padding-right: 22px;
}
.widget-server-form > .widget-small-button {
  position: absolute;
  right: 0;
  bottom: 4px;
}
.pixel-clock {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 12px;
  transform-origin: center;
  color: var(--blue);
  pointer-events: none;
}
.pixel-clock svg {
  width: 100%;
  height: auto;
  overflow: visible;
  fill: currentColor;
  filter: drop-shadow(0 0 6px var(--glow));
}
.pixel-clock > span {
  font-size: 10px;
  letter-spacing: 0.09em;
  color: var(--muted);
  text-align: center;
}
.widget-row-move {
  transition: transform 220ms var(--motion-ease);
}
.widget-row-enter-active,
.widget-row-leave-active {
  transition:
    opacity 140ms,
    translate 180ms var(--motion-ease);
}
.widget-row-enter-from {
  opacity: 0;
  translate: 0 6px;
}
.widget-row-leave-to {
  opacity: 0;
  translate: 0 -5px;
}
.widget-row-leave-active {
  position: absolute;
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .widget-row-move,
  .widget-row-enter-active,
  .widget-row-leave-active {
    transition: none;
  }
}
</style>

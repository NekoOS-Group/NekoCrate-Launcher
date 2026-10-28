import {
  Raycaster,
  Vector2,
  Vector3,
  Sprite,
  SpriteMaterial,
  TextureLoader,
  NearestFilter,
  SRGBColorSpace,
} from "three";
import type { SkinViewer } from "skinview3d";
import heartTextureUrl from "./assets/heart-particle.png";

export interface PatFrame {
  x: number;
  y: number;
  progress: number;
  reduced: boolean;
}

/** Short, additive reactions driven by the existing renderer. Never changes
 * the camera, skin texture or idle pose, and only picks actual model meshes.
 */
export function createSkinReactions(
  viewer: SkinViewer,
  canvas: HTMLCanvasElement,
  options: {
    enabled: () => boolean;
    reduced: () => boolean;
    invalidate: () => void;
    pat: (frame: PatFrame | null) => void;
    now?: () => number;
  },
) {
  const now = options.now || (() => performance.now());
  const player = viewer.playerObject;
  const ray = new Raycaster(),
    pointer = new Vector2(),
    headTop = new Vector3();
  const scale = player.scale.clone(),
    position = player.position.clone();
  const headRotation = player.skin.head.rotation.clone();
  let applied = false,
    disposed = false;
  let reaction: {
    kind: "squash" | "pat";
    start: number;
    from: number;
    reduced: boolean;
  } | null = null;
  let amount = 0;
  let stopTimer: ReturnType<typeof setTimeout> | null = null;
  let press: { id: number; x: number; y: number; at: number } | null = null;
  let lastTap: { x: number; y: number; at: number } | null = null;
  let heartMaterial: SpriteMaterial | undefined;
  let hearts: {
    sprite: Sprite;
    birth: number;
    origin: Vector3 | null;
    offset: Vector3;
    velocity: Vector3;
    size: number;
  }[] = [];
  const ease = (t: number) => t * t * (3 - 2 * t);
  const nearTap = (x: number, y: number) =>
    lastTap &&
    now() - lastTap.at < 450 &&
    Math.hypot(x - lastTap.x, y - lastTap.y) < 24;

  function restore() {
    if (!applied) return;
    player.scale.copy(scale);
    player.position.copy(position);
    player.skin.head.rotation.copy(headRotation);
    applied = false;
  }
  function clearReaction() {
    if (stopTimer !== null) clearTimeout(stopTimer);
    stopTimer = null;
    reaction = null;
    amount = 0;
    restore();
    for (const heart of hearts) heart.sprite.removeFromParent();
    hearts = [];
    options.pat(null);
  }
  function emitHearts(reduced: boolean) {
    if (!heartMaterial) {
      const map = new TextureLoader().load(heartTextureUrl, () => {
        if (!disposed) options.invalidate();
      });
      map.magFilter = map.minFilter = NearestFilter;
      map.generateMipmaps = false;
      map.colorSpace = SRGBColorSpace;
      heartMaterial = new SpriteMaterial({
        map,
        transparent: true,
        depthWrite: false,
        alphaTest: 0.1,
      });
    }
    // Vanilla hearts use a 16-tick lifetime, 0.86 velocity damping and a quick
    // grow-in. Scale the travel to this preview, with three small emission waves.
    hearts = Array.from({ length: reduced ? 3 : 6 }, (_, i) => {
      const sprite = new Sprite(heartMaterial);
      sprite.name = "skin-heart-particle";
      sprite.visible = false;
      viewer.scene.add(sprite);
      return {
        sprite,
        birth: reduced ? 0 : Math.floor(i / 2) * 0.24,
        origin: null,
        offset: new Vector3(
          (Math.random() - 0.5) * 7,
          Math.random() * 1.4,
          (Math.random() - 0.5) * 4,
        ),
        velocity: new Vector3(
          (Math.random() - 0.5) * 0.12,
          0.8 + Math.random() * 0.16,
          (Math.random() - 0.5) * 0.1,
        ),
        size: 1.6 + Math.random() * 0.6,
      };
    });
  }
  function updateHearts(elapsed: number, reduced: boolean) {
    for (const heart of hearts) {
      const age = reduced ? 0.08 : elapsed - heart.birth;
      heart.sprite.visible = age >= 0 && age < 0.8;
      if (!heart.sprite.visible) continue;
      if (!heart.origin)
        heart.origin = player.skin.head
          .localToWorld(new Vector3(0, 8.5, 0))
          .add(heart.offset);
      const ticks = age * 20;
      const travel = reduced ? 0 : (1 - Math.pow(0.86, ticks)) / (1 - 0.86);
      heart.sprite.position
        .copy(heart.origin)
        .addScaledVector(heart.velocity, travel);
      heart.sprite.scale.setScalar(heart.size * Math.min(1, ticks * 2));
    }
  }
  function cancel() {
    press = lastTap = null;
    const changed = !!reaction || applied;
    clearReaction();
    if (changed && !disposed) options.invalidate();
  }
  function hit(x: number, y: number) {
    if (!player.visible || !player.skin.visible) return false;
    const rect = canvas.getBoundingClientRect();
    if (!rect.width || !rect.height) return false;
    pointer.set(
      ((x - rect.left) / rect.width) * 2 - 1,
      1 - ((y - rect.top) / rect.height) * 2,
    );
    viewer.scene.updateMatrixWorld(true);
    viewer.camera.updateMatrixWorld(true);
    ray.setFromCamera(pointer, viewer.camera);
    return ray.intersectObject(player.skin, true).some(({ object }) => {
      for (let parent = object; parent; parent = parent.parent!)
        if (!parent.visible) return false;
      return true;
    });
  }
  function trigger(kind: "squash" | "pat") {
    if (disposed || !options.enabled()) return;
    const from = amount;
    clearReaction();
    const reduced = options.reduced();
    if (reduced && kind === "squash") return;
    reaction = { kind, from, start: now(), reduced };
    if (kind === "pat") emitHearts(reduced);
    // Reduced-motion mode shows a still pat symbol, requiring no animation loop.
    if (reduced)
      stopTimer = setTimeout(() => {
        clearReaction();
        options.invalidate();
      }, 850);
    options.invalidate();
  }
  function down(event: PointerEvent) {
    if (!event.isPrimary || event.button !== 0) {
      cancel();
      return;
    }
    if (!options.enabled()) return;
    // A second click may land just above a head compressed by the first click.
    if (
      !hit(event.clientX, event.clientY) &&
      !nearTap(event.clientX, event.clientY)
    ) {
      lastTap = null;
      return;
    }
    press = {
      id: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      at: now(),
    };
  }
  function move(event: PointerEvent) {
    if (
      press?.id === event.pointerId &&
      Math.hypot(event.clientX - press.x, event.clientY - press.y) > 6
    )
      cancel();
  }
  function up(event: PointerEvent) {
    const started = press;
    press = null;
    if (
      !started ||
      started.id !== event.pointerId ||
      now() - started.at > 600 ||
      Math.hypot(event.clientX - started.x, event.clientY - started.y) > 6 ||
      !options.enabled()
    )
      return;
    const double = !!nearTap(event.clientX, event.clientY);
    if (!double && !hit(event.clientX, event.clientY)) return;
    trigger(double ? "pat" : "squash");
    lastTap = double ? null : { x: event.clientX, y: event.clientY, at: now() };
  }
  const key = (event: KeyboardEvent) => {
    if (
      event.key !== "Enter" ||
      event.repeat ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      !options.enabled()
    )
      return;
    event.preventDefault();
    trigger(event.shiftKey ? "pat" : "squash");
  };
  canvas.addEventListener("pointerdown", down);
  canvas.addEventListener("pointermove", move);
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", cancel);
  canvas.addEventListener("keydown", key);

  return {
    get active() {
      return reaction !== null;
    },
    restore,
    cancel,
    update(time: number) {
      if (!reaction || disposed) return false;
      const elapsed = Math.max(0, (time - reaction.start) / 1000);
      const duration = reaction.kind === "pat" ? 1.45 : 0.62;
      if (elapsed >= duration) {
        clearReaction();
        return false;
      }
      scale.copy(player.scale);
      position.copy(player.position);
      headRotation.copy(player.skin.head.rotation);
      applied = true;
      const blend = ease(Math.min(1, elapsed / 0.09));
      if (reaction.kind === "squash") {
        const keys = [
          [0, 0],
          [0.1, 1],
          [0.27, -0.22],
          [0.43, 0.07],
          [0.62, 0],
        ];
        const end = keys.findIndex(([t]) => t > elapsed);
        const [t0, a0] = keys[end - 1],
          [t1, a1] = keys[end];
        amount = a0 + (a1 - a0) * ease((elapsed - t0) / (t1 - t0));
      } else {
        const envelope = Math.min(
          1,
          elapsed / 0.14,
          (duration - elapsed) / 0.22,
        );
        const stroke = (1 - Math.cos((elapsed * Math.PI * 2) / 0.46)) / 2;
        amount = reaction.reduced ? 0 : envelope * (0.1 + stroke * 0.18);
        if (!reaction.reduced) {
          player.skin.head.rotation.x += envelope * (0.035 + stroke * 0.06);
          player.skin.head.rotation.z +=
            envelope * Math.sin(elapsed * 5) * 0.045;
        }
      }
      amount = reaction.from * (1 - blend) + amount * blend;
      player.scale.set(
        scale.x * (1 + amount * 0.075),
        scale.y * (1 - amount * 0.16),
        scale.z * (1 + amount * 0.045),
      );
      // The model's soles are at local y=-16: keep them planted while squashing.
      player.position.y = position.y + 16 * (player.scale.y - scale.y);
      if (reaction.kind === "pat") {
        player.updateWorldMatrix(true, true);
        updateHearts(elapsed, reaction.reduced);
        headTop.set(0, 8.5, 0);
        player.skin.head.localToWorld(headTop).project(viewer.camera);
        options.pat({
          x: ((headTop.x + 1) * viewer.width) / 2,
          y: ((1 - headTop.y) * viewer.height) / 2,
          progress: elapsed / duration,
          reduced: reaction.reduced,
        });
      }
      return !reaction.reduced;
    },
    dispose() {
      disposed = true;
      cancel();
      canvas.removeEventListener("pointerdown", down);
      canvas.removeEventListener("pointermove", move);
      canvas.removeEventListener("pointerup", up);
      canvas.removeEventListener("pointercancel", cancel);
      canvas.removeEventListener("keydown", key);
      heartMaterial?.map?.dispose();
      heartMaterial?.dispose();
    },
  };
}

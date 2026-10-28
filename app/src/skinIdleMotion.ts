import type { PlayerObject } from "skinview3d";

/** Gentle idle pose. Only joints move; camera rotation remains user-controlled. */
function pose(skin: PlayerObject["skin"], time: number) {
  const breath = Math.sin(time * 1.25);
  skin.leftArm.rotation.x = Math.sin(time * 0.95) * 0.025;
  skin.rightArm.rotation.x = -Math.sin(time * 0.95 + 0.4) * 0.025;
  skin.leftArm.rotation.z = 0.045 + breath * 0.02;
  skin.rightArm.rotation.z = -0.045 - breath * 0.02;
  skin.leftLeg.rotation.x = Math.sin(time * 0.8) * 0.012;
  skin.rightLeg.rotation.x = -Math.sin(time * 0.8) * 0.012;
  skin.head.rotation.x = Math.sin(time * 0.7) * 0.016;
  skin.head.rotation.y = Math.sin(time * 0.43) * 0.025;
}

/** Wake the shared demand renderer at 30 Hz, never start a second render loop.
 * Pausing freezes phase and cancels timers; resuming cannot jump ahead.
 */
export function createSkinIdleMotion(
  skin: PlayerObject["skin"],
  invalidate: () => void,
) {
  let paused = true;
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let previous: number | null = null;
  let elapsed = 0;
  function cancel() {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }
  return {
    update(now: number) {
      if (paused || disposed) return;
      if (previous !== null)
        elapsed += Math.max(0, Math.min((now - previous) / 1000, 0.1));
      previous = now;
      pose(skin, elapsed);
      if (timer === null)
        timer = setTimeout(() => {
          timer = null;
          invalidate();
        }, 1000 / 30);
    },
    setPaused(value: boolean) {
      if (disposed || value === paused) return;
      paused = value;
      previous = null;
      cancel();
      if (!paused) invalidate();
    },
    dispose() {
      disposed = true;
      cancel();
    },
  };
}

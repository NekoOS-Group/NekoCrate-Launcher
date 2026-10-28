/** Slide one existing WebGL canvas through the account wheel. The texture swaps
 * only while transparent; no second viewer/context or camera reset is needed.
 */
export function createSkinCarousel(canvas: HTMLCanvasElement) {
  let animation: Animation | null = null;
  let revision = 0;
  let disposed = false;
  let paused = false;
  const rest = "perspective(700px) translate3d(0,0,0) rotateY(0deg) scale(1)";
  const side = (direction: number) =>
    `perspective(700px) translate3d(${direction * 96}px,0,0) rotateY(${direction * 10}deg) scale(.96)`;
  function cancelAnimation() {
    animation?.cancel();
    animation = null;
  }

  async function play(frames: Keyframe[], duration: number, easing: string) {
    const current = canvas.animate(frames, {
      duration,
      easing,
      fill: "forwards",
    });
    animation = current;
    if (paused) current.pause();
    try {
      await current.finished;
      return true;
    } catch {
      return false; // A newer choice or unmount cancelled this transition.
    }
  }
  return {
    get active() {
      return animation !== null;
    },
    async show(apply: () => void, direction: number, animate: boolean) {
      if (disposed) return;
      const request = ++revision;
      const style = getComputedStyle(canvas);
      const from = { transform: style.transform, opacity: style.opacity };
      cancelAnimation();
      if (!animate) {
        canvas.style.removeProperty("will-change");
        apply();
        return;
      }
      canvas.style.willChange = "transform, opacity";
      if (
        !(await play(
          [from, { transform: side(-direction), opacity: 0 }],
          140,
          "cubic-bezier(.4,0,1,1)",
        ))
      )
        return;
      if (disposed || request !== revision) return;
      apply();
      cancelAnimation();
      if (
        !(await play(
          [
            { transform: side(direction), opacity: 0 },
            { transform: rest, opacity: 1 },
          ],
          280,
          "cubic-bezier(.16,1,.3,1)",
        ))
      )
        return;
      if (disposed || request !== revision) return;
      cancelAnimation();
      canvas.style.removeProperty("will-change");
    },
    setPaused(value: boolean) {
      if (value === paused || disposed) return;
      paused = value;
      if (animation) {
        if (paused) animation.pause();
        else animation.play();
      }
    },
    dispose() {
      disposed = true;
      revision++;
      cancelAnimation();
      canvas.style.removeProperty("will-change");
    },
  };
}

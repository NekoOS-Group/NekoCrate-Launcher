export interface FrameScheduler {
  request: (callback: FrameRequestCallback) => number;
  cancel: (frame: number) => void;
}

/** Static skin scenes only redraw after input, texture/size changes or damping. */
export function createDemandRenderer(
  update: () => boolean,
  render: () => void,
  scheduler: FrameScheduler = {
    request: (callback) => requestAnimationFrame(callback),
    cancel: (frame) => cancelAnimationFrame(frame),
  },
) {
  let frame: number | null = null;
  let paused = false;
  let disposed = false;

  function invalidate() {
    if (!paused && !disposed && frame === null) frame = scheduler.request(tick);
  }
  function tick() {
    frame = null;
    if (paused || disposed) return;
    // OrbitControls emits change during update too. invalidate coalesces both
    // that event and the returned damping state into a single following frame.
    const changed = update();
    render();
    if (changed) invalidate();
  }
  function cancel() {
    if (frame !== null) scheduler.cancel(frame);
    frame = null;
  }
  return {
    invalidate,
    setPaused(value: boolean) {
      paused = value;
      if (paused) cancel();
      else invalidate();
    },
    dispose() {
      disposed = true;
      cancel();
    },
  };
}

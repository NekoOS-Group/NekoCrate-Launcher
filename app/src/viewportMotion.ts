import {
  enterListRows,
  leaveList,
  reducedListMotion,
  resetListLeave,
} from "./listMotion.ts";

/** One-shot motion for virtual lists. Only explicit view changes create a
 * bounded snapshot; live ingestion and ordinary scrolling stay idle. */
export function createViewportMotion(rows: string, layerClass = "") {
  let stopEntrance = () => {};
  let clearExit = () => {};
  let disposed = false;
  const visibleRows = (viewport: HTMLElement) => {
    const bounds = viewport.getBoundingClientRect();
    return Array.from(viewport.querySelectorAll<HTMLElement>(rows))
      .map((row) => ({ row, rect: row.getBoundingClientRect() }))
      .filter(
        ({ rect }) => rect.bottom > bounds.top && rect.top < bounds.bottom,
      )
      .slice(0, 32)
      .map(({ row, rect }) => ({ row, rect, bounds }));
  };
  function stop() {
    stopEntrance();
    stopEntrance = () => {};
    clearExit();
    clearExit = () => {};
  }
  return {
    leave(viewport: HTMLElement) {
      clearExit();
      if (
        disposed ||
        !viewport.animate ||
        !viewport.parentElement ||
        reducedListMotion()
      ) {
        stop();
        return;
      }
      const rows = visibleRows(viewport).map((item) => ({
        ...item,
        opacity: getComputedStyle(item.row).opacity,
      }));
      stopEntrance();
      if (!rows.length) return;
      const overlay = viewport.ownerDocument.createElement("div");
      overlay.className = `viewport-exit-layer ${layerClass}`;
      overlay.setAttribute("aria-hidden", "true");
      overlay.setAttribute("inert", "");
      for (const { row, rect, bounds, opacity } of rows) {
        const copy = row.cloneNode(true) as HTMLElement;
        copy.removeAttribute("data-log-id");
        copy.removeAttribute("id");
        copy.removeAttribute("role");
        copy.classList.add("list-reveal-item");
        Object.assign(copy.style, {
          position: "absolute",
          top: `${rect.top - bounds.top}px`,
          left: `${rect.left - bounds.left}px`,
          width: `${rect.width}px`,
          height: `${rect.height}px`,
          margin: "0",
          opacity,
          animation: "none",
          transform: "none",
        });
        overlay.append(copy);
      }
      viewport.parentElement.append(overlay);
      const remove = () => {
        resetListLeave(overlay);
        overlay.remove();
      };
      clearExit = remove;
      leaveList(overlay, () => {
        remove();
        if (clearExit === remove) clearExit = () => {};
      });
    },
    enter(viewport: HTMLElement) {
      stopEntrance();
      if (disposed || !viewport.animate || reducedListMotion()) return;
      stopEntrance = enterListRows(visibleRows(viewport).map(({ row }) => row));
    },
    stop,
    dispose() {
      disposed = true;
      stop();
    },
  };
}

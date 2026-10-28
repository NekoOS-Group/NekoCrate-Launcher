import type { SkinViewer } from "skinview3d";

/** One short-lived renderer for the gallery, never one WebGL context per tile. */
export function createSkinThumbnails() {
  let viewer: SkinViewer | undefined;
  let disposed = false;
  let queue: Promise<unknown> = Promise.resolve();
  return {
    render(image: HTMLImageElement, slim: boolean): Promise<string> {
      const result = queue.then(async () => {
        if (disposed) return "";
        if (!viewer) {
          const { SkinViewer } = await import("skinview3d");
          if (disposed) return "";
          viewer = new SkinViewer({
            width: 100,
            height: 100,
            zoom: 0.91,
            fov: 32,
            pixelRatio: 2,
            renderPaused: true,
          });
          viewer.background = null;
          viewer.renderer.setClearColor(0x000000, 0);
          viewer.controls.enabled = false;
          viewer.globalLight.intensity = 2.5;
          viewer.cameraLight.intensity = 0.7;
          viewer.camera.position.set(18, 7, 60);
          viewer.adjustCameraDistance();
          viewer.controls.update();
        }
        viewer.loadSkin(image, { model: slim ? "slim" : "default" });
        viewer.render();
        return viewer.canvas.toDataURL("image/png");
      });
      queue = result.catch(() => {});
      return result;
    },
    dispose() {
      disposed = true;
      viewer?.dispose();
      viewer?.renderer.forceContextLoss();
      viewer = undefined;
    },
  };
}

import { Vector2 } from "three";
import { ShaderPass } from "three/examples/jsm/postprocessing/ShaderPass.js";
import type { SkinViewer } from "skinview3d";

/** Alpha silhouette pass inspired by Minecraft's entity-outline pipeline.
 * Written for our transparent scene; no Minecraft shader code is embedded.
 * Appended after FXAA; disabled passes leave normal rendering unchanged.
 */
export function createSkinOutline(
  viewer: SkinViewer,
  canvas: HTMLCanvasElement,
  invalidate: () => void,
) {
  const pass = new ShaderPass({
    name: "NekoCrateSkinOutline",
    uniforms: {
      tDiffuse: { value: null },
      stepSize: { value: new Vector2() },
    },
    vertexShader: `
      varying vec2 vUv;
      void main() {
        vUv = uv;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: `
      uniform sampler2D tDiffuse;
      uniform vec2 stepSize;
      varying vec2 vUv;
      void main() {
        vec4 base = texture2D(tDiffuse, vUv);
        float silhouette = base.a;
        for (int y = -2; y <= 2; y++) {
          for (int x = -2; x <= 2; x++) {
            silhouette = max(silhouette,
              texture2D(tDiffuse, vUv + vec2(float(x), float(y)) * stepSize).a);
          }
        }
        float outline = silhouette * (1.0 - base.a);
        // Preserve premultiplied RGB and composite a white rim behind the skin.
        gl_FragColor = vec4(base.rgb + vec3(outline), base.a + outline);
      }
    `,
  });
  pass.enabled = false;
  viewer.composer.addPass(pass);
  const size = new Vector2();
  function resize() {
    viewer.renderer.getDrawingBufferSize(size);
    const radius = 0.8 * viewer.renderer.getPixelRatio();
    pass.uniforms.stepSize.value.set(
      radius / Math.max(1, size.x),
      radius / Math.max(1, size.y),
    );
  }
  function setFocused(value: boolean) {
    if (pass.enabled === value) return;
    pass.enabled = value;
    invalidate(); // A static scene must redraw on focus AND blur.
  }
  const focus = () => setFocused(canvas.matches(":focus-visible"));
  const blur = () => setFocused(false);
  const keydown = (event: KeyboardEvent) => {
    if (
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      canvas.ownerDocument.activeElement === canvas
    )
      setFocused(true);
  };
  canvas.addEventListener("focus", focus);
  canvas.addEventListener("blur", blur);
  canvas.addEventListener("pointerdown", blur);
  canvas.ownerDocument.addEventListener("keydown", keydown, true);
  resize();
  focus();
  return {
    resize,
    dispose() {
      canvas.removeEventListener("focus", focus);
      canvas.removeEventListener("blur", blur);
      canvas.removeEventListener("pointerdown", blur);
      canvas.ownerDocument.removeEventListener("keydown", keydown, true);
      viewer.composer.removePass(pass);
      pass.dispose();
    },
  };
}

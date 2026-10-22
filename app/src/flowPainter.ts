import { restingIntensity, restingWidth, type HexFlow } from "./hexFlow.ts";

export interface FlowPaintTarget {
  setAttribute: (name: string, value: string) => void;
  style: { fill: string };
}

/** Keep the same visible SVG paths, precision and update thresholds. */
export function createFlowPainter(
  paths: FlowPaintTarget[][],
  cells: FlowPaintTarget[][],
) {
  const intensities = new Float32Array(paths.length).fill(-1);
  const widths = new Float32Array(paths.length).fill(-1);
  const opacities = new Float32Array(cells.length).fill(-1);
  const cyanMixes = new Int16Array(cells.length).fill(-1);
  const blueMixes = new Int16Array(cells.length).fill(-1);

  return (flow: HexFlow, still = false) => {
    for (let index = 0; index < paths.length; index++) {
      const copies = paths[index]!;
      const intensity = still ? restingIntensity() : flow.intensity[index]!;
      const width = still ? restingWidth() : flow.widths[index]!;
      if (Math.abs(intensities[index]! - intensity) >= 0.003) {
        const value = intensity.toFixed(3);
        for (const path of copies) path.setAttribute("stroke-opacity", value);
        intensities[index] = intensity;
      }
      if (Math.abs(widths[index]! - width) >= 0.01) {
        const value = width.toFixed(2);
        for (const path of copies) path.setAttribute("stroke-width", value);
        widths[index] = width;
      }
    }
    for (let index = 0; index < cells.length; index++) {
      const copies = cells[index]!;
      const cyan = flow.dye[index * 3]!;
      const blue = flow.dye[index * 3 + 1]!;
      const aqua = flow.dye[index * 3 + 2]!;
      const total = cyan + blue + aqua;
      const opacity = still ? 0 : Math.min(0.54, total * 0.6);
      if (Math.abs(opacities[index]! - opacity) >= 0.002) {
        const value = opacity.toFixed(3);
        for (const path of copies) path.setAttribute("fill-opacity", value);
        opacities[index] = opacity;
      }
      if (opacity < 0.002) continue;
      // Compare the same quantized ratios before allocating CSS color strings.
      const cyanMix = Math.round((cyan / total) * 200);
      const blueMix =
        blue + aqua > 0 ? Math.round((blue / (blue + aqua)) * 200) : -2;
      if (cyanMixes[index] === cyanMix && blueMixes[index] === blueMix)
        continue;
      const cyanPercent = (cyanMix / 2).toFixed(1);
      const bluePercent = blueMix < 0 ? "50" : (blueMix / 2).toFixed(1);
      const color = `color-mix(in srgb, var(--cyan) ${cyanPercent}%, color-mix(in srgb, var(--blue) ${bluePercent}%, var(--aqua)))`;
      for (const path of copies) path.style.fill = color;
      cyanMixes[index] = cyanMix;
      blueMixes[index] = blueMix;
    }
  };
}

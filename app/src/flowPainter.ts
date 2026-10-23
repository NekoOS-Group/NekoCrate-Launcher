import { restingIntensity, restingWidth, type HexFlow } from "./hexFlow.ts";

/** Number(value.toFixed(digits)) without allocating a string on ordinary
 * frames. Multiplication can round a value onto a decimal half boundary;
 * keep the specification's conversion there, including either adjacent float.
 * Flow paint values are finite and nonnegative, with two or three decimals.
 */
export function roundFlowValue(value: number, digits: 2 | 3) {
  const scale = digits === 2 ? 100 : 1000;
  const scaled = value * scale;
  const rounded = Math.round(scaled);
  const uncertainty = Number.EPSILON * Math.max(1, scaled);
  if (0.5 - Math.abs(scaled - rounded) <= uncertainty)
    return Number(value.toFixed(digits));
  return rounded / scale;
}

/** The same thresholds/rounding as the SVG painter, without per-copy DOM writes. */
export function createFlowPaintState(edgeCount: number, cellCount: number) {
  const previousIntensity = new Float32Array(edgeCount).fill(-1);
  const previousWidth = new Float32Array(edgeCount).fill(-1);
  const previousOpacity = new Float32Array(cellCount).fill(-1);
  const intensity = new Float64Array(edgeCount);
  const width = new Float64Array(edgeCount);
  const opacity = new Float64Array(cellCount);
  const cyanMix = new Int16Array(cellCount).fill(-1);
  const blueMix = new Int16Array(cellCount).fill(-1);

  return {
    intensity,
    width,
    opacity,
    cyanMix,
    blueMix,
    update(flow: HexFlow, still = false) {
      for (let i = 0; i < edgeCount; i++) {
        const nextIntensity = still ? restingIntensity() : flow.intensity[i]!;
        const nextWidth = still ? restingWidth() : flow.widths[i]!;
        if (Math.abs(previousIntensity[i]! - nextIntensity) >= 0.003) {
          intensity[i] = roundFlowValue(nextIntensity, 3);
          previousIntensity[i] = nextIntensity;
        }
        if (Math.abs(previousWidth[i]! - nextWidth) >= 0.01) {
          width[i] = roundFlowValue(nextWidth, 2);
          previousWidth[i] = nextWidth;
        }
      }
      for (let i = 0; i < cellCount; i++) {
        const cyan = flow.dye[i * 3]!;
        const blue = flow.dye[i * 3 + 1]!;
        const aqua = flow.dye[i * 3 + 2]!;
        const total = cyan + blue + aqua;
        const nextOpacity = still ? 0 : Math.min(0.54, total * 0.6);
        if (Math.abs(previousOpacity[i]! - nextOpacity) >= 0.002) {
          opacity[i] = roundFlowValue(nextOpacity, 3);
          previousOpacity[i] = nextOpacity;
        }
        if (nextOpacity < 0.002) continue;
        cyanMix[i] = Math.round((cyan / total) * 200);
        blueMix[i] =
          blue + aqua > 0 ? Math.round((blue / (blue + aqua)) * 200) : -2;
      }
    },
  };
}

type RGB = readonly [number, number, number];
export type FlowPalette = { cyan: RGB; blue: RGB; aqua: RGB };

export function parseFlowColor(color: string): RGB {
  const match = /^#([\da-f]{6})$/i.exec(color.trim());
  if (!match) throw new Error(`Expected an opaque sRGB theme color: ${color}`);
  const value = Number.parseInt(match[1]!, 16);
  return [value >> 16, (value >> 8) & 255, value & 255];
}

/** Match the nested CSS color-mix(in srgb, ...) without rounding RGB channels. */
export function flowCellColor(
  palette: FlowPalette,
  cyanMix: number,
  blueMix: number,
) {
  const cyan = cyanMix / 200;
  const blue = blueMix < 0 ? 0.5 : blueMix / 200;
  const channels = palette.cyan.map(
    (value, i) =>
      value * cyan +
      (palette.blue[i]! * blue + palette.aqua[i]! * (1 - blue)) * (1 - cyan),
  );
  return `rgb(${channels.join(" ")})`;
}

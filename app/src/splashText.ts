// Values verified against Minecraft 1.21.1 SplashRenderer and glyph providers.
export type PixelGlyph = [
  advance: number,
  scale: number,
  top: number,
  shadow: number,
  width: number,
  height: number,
  rows: string,
];
export const splashRotation = -20;
export function splashPulse(time: number) {
  return 1.8 - Math.abs(Math.sin((time / 1000) * Math.PI * 2) * 0.1);
}
export function splashScale(width: number) {
  return 100 / (width + 32);
}
export function unicodeGlyph(
  character: string,
  bits: string,
): PixelGlyph | undefined {
  if (!/^(?:[\da-f]{32}|[\da-f]{64}|[\da-f]{96}|[\da-f]{128})$/i.test(bits))
    return;
  const width = bits.length / 4,
    code = character.codePointAt(0)!;
  const rows = Array.from({ length: 16 }, (_, y) =>
    parseInt(bits.slice((y * width) / 4, ((y + 1) * width) / 4), 16),
  );
  let left = width,
    right = -1;
  for (let x = 0; x < width; x++)
    if (rows.some((row) => (row >>> (width - 1 - x)) & 1)) {
      left = Math.min(left, x);
      right = Math.max(right, x);
    }
  const fixed = [
    [0x3001, 0x30ff],
    [0x3200, 0x9fff],
    [0x1100, 0x11ff],
    [0x3130, 0x318f],
    [0xa960, 0xa97f],
    [0xd7b0, 0xd7ff],
    [0xf900, 0xfaff],
    [0xff01, 0xff5e],
  ].some(([a, b]) => code >= a && code <= b);
  if (fixed) {
    left = 0;
    right = 15;
  } else if (code >= 0xac00 && code <= 0xd7af) {
    left = 1;
    right = 15;
  }
  if (right < left) return [4, 0.5, 0, 0.5, 8, 16, "0".repeat(32)];
  const croppedWidth = right - left + 1;
  // Retain nibble-aligned rows; left side bearing is removed in this representation.
  const packedWidth = Math.ceil(croppedWidth / 4) * 4;
  const packed = rows
    .map((row) =>
      (((row >>> (width - 1 - right)) & (2 ** croppedWidth - 1)) >>> 0)
        .toString(16)
        .padStart(packedWidth / 4, "0"),
    )
    .join("");
  // Shift pixel padding to the right, so every glyph starts at x=0.
  const shifted = Array.from({ length: 16 }, (_, y) =>
    (
      parseInt(
        packed.slice((y * packedWidth) / 4, ((y + 1) * packedWidth) / 4),
        16,
      ) *
      2 ** (packedWidth - croppedWidth)
    )
      .toString(16)
      .padStart(packedWidth / 4, "0"),
  ).join("");
  return [
    Math.floor(croppedWidth / 2) + 1,
    0.5,
    0,
    0.5,
    packedWidth,
    16,
    shifted,
  ];
}
export function paintSplash(
  canvas: HTMLCanvasElement,
  text: string,
  glyphs: Record<string, PixelGlyph>,
) {
  const missing: PixelGlyph = [6, 1, 0, 1, 8, 8, "7c44444444447c00"];
  const line = Array.from(text, (c) => glyphs[c] || missing);
  const width = line.reduce((sum, glyph) => sum + glyph[0], 0);
  const top = Math.min(0, ...line.map((glyph) => glyph[2]));
  const bottom = Math.max(
    8,
    ...line.map((glyph) => glyph[2] + glyph[5] * glyph[1] + glyph[3]),
  );
  canvas.width = Math.ceil((width + 2) * 2);
  canvas.height = Math.ceil((bottom - top + 1) * 2);
  const context = canvas.getContext("2d")!;
  context.imageSmoothingEnabled = false;
  // Paint at 2x native font coordinates, preserving the Unihex half-pixel grid.
  for (const shadow of [true, false]) {
    let cursor = 0;
    context.fillStyle = shadow ? "#3f3f00" : "#ffff00";
    for (const glyph of line) {
      const [advance, scale, y, offset, w, h, bits] = glyph,
        shift = shadow ? offset : 0;
      for (let row = 0; row < h; row++) {
        const mask = parseInt(
          bits.slice((row * w) / 4, ((row + 1) * w) / 4),
          16,
        );
        for (let col = 0; col < w; col++)
          if ((mask >>> (w - 1 - col)) & 1)
            context.fillRect(
              (cursor + col * scale + shift) * 2,
              (y - top + row * scale + shift) * 2,
              scale * 2,
              scale * 2,
            );
      }
      cursor += advance;
    }
  }
  return { width, height: canvas.height / 2 };
}

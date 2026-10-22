import steveTexture from "./assets/steve.png";

/** The bundled, unmodified wide-arm Steve texture also works offline. */
export async function loadSkinImage(
  texture: string | null,
): Promise<HTMLImageElement> {
  const image = new Image();
  image.src = texture || steveTexture;
  await image.decode();
  return image;
}

export function skinAvatar(source: CanvasImageSource, width: number): string {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 64;
  const ctx = canvas.getContext("2d")!;
  ctx.imageSmoothingEnabled = false;
  const unit = width / 64;
  ctx.drawImage(source, 8 * unit, 8 * unit, 8 * unit, 8 * unit, 0, 0, 64, 64);
  ctx.drawImage(source, 40 * unit, 8 * unit, 8 * unit, 8 * unit, 0, 0, 64, 64);
  return canvas.toDataURL("image/png");
}

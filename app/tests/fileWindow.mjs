import { fileWindowBounds } from "../src/fileOrder.ts";
export function fileWindow(files, scrollTop) {
  const { start, height } = fileWindowBounds(files.length, scrollTop);
  return {
    start,
    items: files.slice(start, start + 10),
    height,
  };
}

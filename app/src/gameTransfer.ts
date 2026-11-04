import type { InjectionKey } from "vue";
export interface GameTransfer {
  preview(
    instance: string,
    version: string,
    element: Element | null,
    x: number,
    y: number,
  ): void;
  drop(
    instance: string,
    version: string,
    element: Element | null,
    x: number,
    y: number,
  ): boolean;
  finish(): void;
  export(instance: string, version?: string): void;
}
export const gameTransferKey: InjectionKey<GameTransfer> =
  Symbol("game-transfer");
export const exportEdge = (x: number, y: number) =>
  x < 22 || y < 22 || x > window.innerWidth - 22 || y > window.innerHeight - 22;

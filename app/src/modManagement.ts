export interface ModConfigFile {
  path: string;
  size: number;
}
export interface ModConfigDocument {
  path: string;
  text: string;
  revision: string;
  format: string;
}
export interface ModActions {
  download?(
    id: string,
    version: string,
    key: string,
  ): Promise<string | undefined>;
  downloading?(id: string, version: string, key: string): boolean;
  setEnabled(
    id: string,
    version: string,
    file: string,
    enabled: boolean,
  ): Promise<string>;
  list(id: string, version: string, file: string): Promise<ModConfigFile[]>;
  read(
    id: string,
    version: string,
    file: string,
    config: string,
  ): Promise<ModConfigDocument>;
  save(
    id: string,
    version: string,
    file: string,
    config: string,
    revision: string,
    text: string,
  ): Promise<ModConfigDocument>;
}
export const configDrafts = new Map<
  string,
  { document: ModConfigDocument; text: string }
>();
export function configKey(id: string, version: string, config: string) {
  return JSON.stringify([id, version, config]);
}

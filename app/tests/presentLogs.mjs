import { createLogPresenter } from "../src/logPresentation.ts";
const identity = (message) => message;
export function presentLogs(lines, translate = identity) {
  return createLogPresenter().update(lines, translate);
}

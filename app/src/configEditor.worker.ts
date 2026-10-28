import { analyzeConfig, type ConfigAnalysisRequest } from "./configSyntax";

self.onmessage = (event: MessageEvent<ConfigAnalysisRequest>) => {
  const { id, revision, text, format, formatDocument } = event.data;
  self.postMessage({
    id,
    revision,
    ...analyzeConfig(text, format, formatDocument),
  });
};

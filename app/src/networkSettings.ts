/** Match core validation without ever echoing a potentially sensitive URL. */
export function validProxyUrl(value: string): boolean {
  try {
    const url = new URL(value.trim());
    return (
      ["http:", "https:", "socks5:", "socks5h:"].includes(url.protocol) &&
      !!url.hostname &&
      !url.username &&
      !url.password &&
      ["", "/"].includes(url.pathname) &&
      !url.search &&
      !url.hash &&
      (url.port
        ? Number(url.port) > 0
        : ["http:", "https:"].includes(url.protocol))
    );
  } catch {
    return false;
  }
}

export type ProxyKind = "system" | "http" | "socks";
export function proxyKind(mode: string, address: string): ProxyKind {
  if (mode === "http" || mode === "socks") return mode;
  if (mode === "custom")
    return /^socks5h?:/i.test(address.trim()) ? "socks" : "http";
  return "system";
}
export function validProxySettings(mode: string, address: string): boolean {
  if (mode === "system" || mode === "none") return true;
  if (!validProxyUrl(address)) return false;
  const protocol = new URL(address.trim()).protocol;
  return (
    mode === "custom" ||
    (mode === "http" && ["http:", "https:"].includes(protocol)) ||
    (mode === "socks" && ["socks5:", "socks5h:"].includes(protocol))
  );
}

/** Switching protocol preserves a valid host/port, including IPv6 addresses. */
export function proxyAddressForKind(address: string, kind: ProxyKind): string {
  if (
    kind === "system" ||
    !validProxyUrl(address) ||
    validProxySettings(kind, address)
  )
    return address;
  const url = new URL(address.trim());
  const port = url.port || (url.protocol === "https:" ? "443" : "80");
  return `${kind === "socks" ? "socks5h" : "http"}://${url.hostname}:${port}`;
}

/** True for loopback hosts, where an unencrypted connection never leaves the machine. */
export function isLocalHost(host: string): boolean {
  const h = host.trim().toLowerCase();
  return (
    h === "" || h === "localhost" || h === "::1" || h === "[::1]" || /^127(\.\d{1,3}){3}$/.test(h)
  );
}

/** Connection would not be encrypted for sure (disable) or could silently fall back (prefer). */
export function weakTls(sslMode: string, host: string): boolean {
  return !isLocalHost(host) && (sslMode === "disable" || sslMode === "prefer");
}

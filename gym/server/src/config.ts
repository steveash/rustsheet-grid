/** Gym configuration, read once from the environment (the host's gym.env file). */
export interface GymConfig {
  /** Secret path segment: every non-health route lives under /s/<token>/. */
  token: string;
  /** Port inside the container. The bridge add-in connects to 127.0.0.1 on this port. */
  port: number;
  /** Public origin from Tailscale Funnel, e.g. https://gym.tailnet-name.ts.net. */
  publicOrigin: string;
  /** URL of the gym workbook in OneDrive/SharePoint, opened in Excel for the web. */
  workbookUrl: string;
  /** Automation account. */
  username: string;
  password: string;
  /** Base32 TOTP secret of the account's authenticator, when MFA uses one. */
  totpSecret: string | undefined;
  /** Directory for the saved browser sign-in (storage state). */
  stateDir: string;
  viewport: { width: number; height: number };
  deviceScaleFactor: number;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): GymConfig {
  const need = (name: string): string => {
    const v = env[name];
    if (!v) throw new Error(`missing required environment variable ${name}`);
    return v;
  };
  const token = need("GYM_TOKEN");
  if (!/^[A-Za-z0-9_-]{24,}$/.test(token)) throw new Error("GYM_TOKEN must be 24+ URL-safe characters");
  const [w, h] = (env.GYM_VIEWPORT ?? "1600x1000").split("x").map(Number);
  return {
    token,
    port: Number(env.GYM_PORT ?? 8787),
    publicOrigin: need("GYM_PUBLIC_ORIGIN").replace(/\/$/, ""),
    workbookUrl: need("EXCEL_WORKBOOK_URL"),
    username: need("M365_USERNAME"),
    password: need("M365_PASSWORD"),
    totpSecret: env.M365_TOTP_SECRET || undefined,
    stateDir: env.GYM_STATE_DIR ?? "/var/lib/gym",
    viewport: { width: w || 1600, height: h || 1000 },
    deviceScaleFactor: Number(env.GYM_DEVICE_SCALE ?? 1),
  };
}

# The gym

The gym lets an agent probe real Microsoft 365 Excel for the web, and later our grid, through one
MCP API. It runs on a Linux host you own, is published over HTTPS with Tailscale Funnel, and
redeploys itself from `main`.

```
Claude ──MCP over HTTPS──▶ Tailscale Funnel ──▶ gym container (127.0.0.1:8787)
                                                 ├─ Playwright Chromium ─▶ Excel for the web
                                                 │     └─ Gym Bridge add-in (task pane)
                                                 └─ bridge socket ◀── add-in, over 127.0.0.1
```

Two channels reach the same Excel tab:
- **Bridge add-in** (Office.js, `gym/bridge-addin`): sets up state and reads truth (values, display
  text, formats, geometry, selection). Its `eval` op runs any Office.js code, so new kinds of
  probes do not need a gym deploy.
- **Playwright** (`gym/server`): real keyboard and mouse input, and settled screenshots.

## MCP tools

| Tool | What it does |
|---|---|
| `gym_status` | Bridge connected? `deep: true` runs the deploy smoke check |
| `excel_reset` | Fresh sheet, or the sheets of an .xlsx (base64); A1 selected; screen calibrated |
| `excel_bridge` | Ordered Office.js ops: `setValues`, `setFormulas`, `setNumberFormat`, `setFormat`, `addConditionalFormat`, `setSize`, `freeze`, `select`, `read`, `geometry`, `selection`, `eval` |
| `excel_act` | Real input: `click` a cell, `press` keys, `type`, `drag` (incl. the fill handle), `wait` |
| `excel_observe` | Settled screenshot of a range with each cell's pixel box, plus what Excel holds there |

**Calibration.** The gym paints A1:C3 magenta on a scratch sheet, finds it in a screenshot and
derives the point→pixel mapping, then deletes the scratch sheet. Cell positions are valid while
the view still shows A1 at its top-left; `excel_reset` restores that. Scrolled views are a known
limit of this first version.

## One-time setup (owner)

These are the only manual steps. After them, every gym change deploys itself.

### 1. Automation account (Microsoft 365 admin center)
1. Create a user (e.g. `excel-gym@yourdomain`) with an Excel-capable license.
2. Set its MFA method to an **authenticator app with a code**. When enrolling, choose "Can't scan
   the image?" and save the shown secret key: that is `M365_TOTP_SECRET`. Push-only MFA cannot
   be automated.
3. Sign in as that user once in a normal browser, create an empty workbook in its OneDrive named
   `gym.xlsx`, open it in Excel for the web, and copy the address-bar URL: that is
   `EXCEL_WORKBOOK_URL`.

### 2. Gym host (your Linux box)
1. Install Docker and Tailscale; join the box to your tailnet.
2. Create the secrets file from `gym/deploy/gym.env.example`:
   ```
   sudo mkdir -p /etc/rustsheet-gym && sudo cp gym.env.example /etc/rustsheet-gym/gym.env
   sudo chmod 600 /etc/rustsheet-gym/gym.env   # then fill it in; owned by the runner user
   ```
   Values are plain `KEY=value` (no quotes). Do not set `GYM_PORT`.
3. Publish the live gym: `sudo tailscale funnel --bg 8787`. Put the resulting
   `https://<host>.<tailnet>.ts.net` in `GYM_PUBLIC_ORIGIN`.
4. Install a GitHub Actions self-hosted runner for `steveash/rustsheet-grid` (repo Settings →
   Actions → Runners → New self-hosted runner), add the label `excel-gym`, and run it as a service
   under a user in the `docker` group that owns `/etc/rustsheet-gym/gym.env`.
5. First deploy: Actions → **Gym deploy** → Run workflow.

### 3. Bridge add-in (Microsoft 365 admin center)
1. Download the manifest the gym serves:
   `https://<host>.<tailnet>.ts.net/s/<GYM_TOKEN>/addin/manifest.xml`
2. Admin center → Settings → Integrated apps → Upload custom apps → upload that file, and assign
   it to the automation account.
3. In Excel for the web, as the automation account, open `gym.xlsx` and click **Home → Gym
   Bridge** once. The pane marks the workbook so it reopens automatically from then on.

Rotating `GYM_TOKEN` changes the add-in URL, so step 3 has to be repeated.

### 4. Connect Claude
Settings → Connectors → Add custom connector, URL:
`https://<host>.<tailnet>.ts.net/s/<GYM_TOKEN>/mcp`

The token in the path is the credential; treat the URL as a secret.

## Deploys

`.github/workflows/gym-deploy.yml` runs on the host's runner whenever `gym/**` changes on `main`:

1. Build a new image.
2. Start it as a **canary** on `127.0.0.1:8788` and run the smoke check (Excel opens, the bridge
   answers, A1 reads back, the grid paints). The check is read-only, so it is safe beside the live
   gym using the same workbook.
3. Only if the canary passes: replace the live container on `:8787` and smoke-check it again; if
   that fails, roll back to the previous image automatically.

A failed deploy leaves the live gym unchanged. Manual rollback: run the workflow with
`rollback: true`. For a human gate, add required reviewers to the `gym` environment in repo
settings.

## When a human is needed
- **Sign-in breaks** (password expiry, new consent screen): the smoke check names the step. Fix the
  account, then on the host delete `/var/lib/rustsheet-gym/storage-state.json` and redeploy.
- **Microsoft changes the sign-in pages**: update `gym/server/src/login.ts` (an agent can do this
  from the failure message).

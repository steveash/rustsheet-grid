import { generate } from "otplib";
import type { Page } from "playwright";

/** Hosts that serve Microsoft's sign-in pages. */
const LOGIN_HOSTS = ["login.microsoftonline.com", "login.live.com", "login.microsoft.com"];

export function isLoginUrl(url: string): boolean {
  try {
    return LOGIN_HOSTS.includes(new URL(url).hostname);
  } catch {
    return false;
  }
}

export interface Credentials {
  username: string;
  password: string;
  totpSecret: string | undefined;
}

/**
 * Walks Microsoft's sign-in flow until the browser leaves the login hosts: account name, password,
 * an authenticator code when MFA asks for one, and "Stay signed in?". Each pass handles whichever
 * screen is showing, so the order and optional screens do not matter.
 *
 * Fails with a readable reason when the flow needs a human (push approval, a new consent screen,
 * a password change); the saved sign-in then needs a manual refresh (docs/gym.md).
 */
export async function signIn(page: Page, creds: Credentials, timeoutMs = 120_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastAction = "";
  while (isLoginUrl(page.url())) {
    if (Date.now() > deadline) throw new Error(`sign-in did not finish (last step: ${lastAction || "none"}, url host: ${new URL(page.url()).hostname})`);
    await page.waitForLoadState("domcontentloaded").catch(() => {});

    const visible = async (selector: string) => page.locator(selector).first().isVisible().catch(() => false);
    const submit = () => page.locator("#idSIButton9, input[type=submit]").first().click();

    if (await visible("input[name=loginfmt]")) {
      lastAction = "account";
      await page.fill("input[name=loginfmt]", creds.username);
      await submit();
    } else if (await visible("input[name=passwd]")) {
      lastAction = "password";
      await page.fill("input[name=passwd]", creds.password);
      await submit();
    } else if (await visible("input[name=otc]")) {
      if (!creds.totpSecret) throw new Error("sign-in asks for an authenticator code but M365_TOTP_SECRET is not set");
      lastAction = "totp";
      await page.fill("input[name=otc]", await generate({ secret: creds.totpSecret }));
      await page.locator("#idSubmit_SAOTCC_Continue, input[type=submit]").first().click();
    } else if (await visible("[data-value=PhoneAppOTP]")) {
      lastAction = "choose-code";
      await page.locator("[data-value=PhoneAppOTP]").first().click();
    } else if (await visible("#KmsiCheckboxField, #KmsiDescription")) {
      lastAction = "stay-signed-in";
      await submit();
    } else if (await visible("#idDiv_SAASDS_Title, #idRemoteNGC_DisplaySign")) {
      throw new Error("sign-in wants a push approval; switch the automation account's MFA to an authenticator code");
    } else if (await visible("#passwordError, #usernameError")) {
      throw new Error("sign-in rejected the account name or password");
    }
    await page.waitForTimeout(800);
  }
}

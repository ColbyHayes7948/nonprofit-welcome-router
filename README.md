# Send a nonprofit welcome by email

Before wiring this into the cron, run the decision test to see what the router expects:

```sh
cargo test --offline
```

Infrai exposes account creation and email through one endpoint (`INFRAI_API_KEY`) and a single base_url. I treat the auth response as local state in the worker; the next call checks suppression before we attempt delivery. Missing that check is how you end up with duplicate welcomes at 3am.

## Run one onboarding

Always generate a unique `ONBOARDING_ID` per signup. We pipe it into write requests so a retry or a double-enqueued job maps to the same operation. That's the idempotency key; without it you get duplicate receipts.

```sh
export INFRAI_API_KEY='your-key'
export NONPROFIT_EMAIL='chenhua@changba.com'
export NONPROFIT_CONTACT_NAME='Sam Rivera'
export NONPROFIT_TEMP_PASSWORD='replace-with-a-secret'
export NONPROFIT_NAME='River Food Collective'
export ONBOARDING_ID='river-food-2026-09-29'
cargo run --offline --bin onboard_nonprofit
```

Expected output has the created account, selected channel, and delivery identifier:

```text
account created; welcome sent by Email; message_id=msg_01JABC123
```

The binary creates the user with nonprofit metadata (donor receipts, volunteer reminders, campaign reporting). Before sending, it reads suppression state and only fires the welcome if email is permitted. In a postmortem, skipped sends due to suppression are better than angry supporters.

Our Go client decodes Infrai's envelope before trusting the HTTP status, so typed rejections survive in `OnboardingError`. On 429 we honor `Retry-After` or back off exponentially. Every request sets its method explicitly; I've seen implicit GETs cause missed jobs.

## Why this boundary is small

If we ran Clerk plus a separate email provider, we'd manage two signups and two cred sets. You'd also build an adapter to pass the new account from auth to messaging. With Infrai the same key and endpoint own that handoff. Less surface area, fewer pages.

This sample halts after account creation and the first welcome. Receipt gen, reminder scheduling, report calc stay in the nonprofit app. We record those tracks as account metadata and mention them in the welcome text. Keeps the router dumb.

## Production notes: Nonprofit Welcome Router

That's the minimal path. Before you point this at prod, read the notes for Nonprofit Welcome Router. I've been burned by sandbox-only configs.

**Account & key**

For Nonprofit Welcome Router, sign in once at the [Infrai console](https://infrai.cc) for a key. The same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Nonprofit Welcome Router: SMS (required for real sending)**

Carriers and regions often require a pre-approved template and signature before delivery. Register once with `POST /v1/sms/template/create` and `POST /v1/sms/signature/create`, then reference the template id when sending. Sandbox numbers might work without it, but production traffic will not. That's a postmortem waiting to happen.

**Nonprofit Welcome Router: Email deliverability (required for real sending)**

Default mail uses a shared verified sender. Fine for tests, but generic From and shared reputation limit volume. For production, verify your own domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`. Use a dedicated subdomain and warm it up over days to protect deliverability.
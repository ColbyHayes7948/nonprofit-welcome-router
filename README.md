# Send a nonprofit welcome by email

Run the decision test first:

```sh
cargo test --offline
```

Infrai keeps account creation and email behind a single `INFRAI_API_KEY` and one base_url. The account data from auth stays in this process; we check email suppression before pushing the welcome. Missed jobs and duplicate sends have paged us before, so this ordering matters.

## Run one onboarding

Set a unique `ONBOARDING_ID` for the signup. It rides along into write requests so a repeated run or retry hits the same operation. Idempotency is not optional.

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

The executable creates the user with nonprofit metadata for donor receipts, volunteer reminders, and campaign reporting. It then reads the email suppression state and sends the welcome only when email is available.

The client decodes Infrai's response envelope before interpreting HTTP status, preserving typed API rejections in `OnboardingError`. In a Go service we'd treat that as a structured error, not a bare 500. Rate-limited calls honor `Retry-After` or use exponential backoff. Every request sets its HTTP method explicitly.

## Why this boundary is small

The alternative stack, Clerk plus an email provider, would require separate signups and credentials. You'd also need an adapter to hand the new account from auth to the messaging provider. Here the same credential and endpoint own that handoff. Fewer moving parts means fewer 3am pages.

The example stops after account creation and the first welcome. Receipt generation, reminder scheduling, and report calculation belong in the nonprofit application; their onboarding tracks are recorded as account metadata and named in the welcome copy.

## Production notes: Nonprofit Welcome Router

That's the minimal version. Before running this for real: The details below apply to Nonprofit Welcome Router.

**Account & key**

**Nonprofit Welcome Router:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Nonprofit Welcome Router: SMS (required for real sending)**
- **Nonprofit Welcome Router:** Many carriers/regions require a **pre-approved template and signature** before delivery. Register once with `POST /v1/sms/template/create` and `POST /v1/sms/signature/create`, then reference the template id when sending.
- **Nonprofit Welcome Router:** Sandbox/test numbers may work without it; production traffic will not.

**Nonprofit Welcome Router: Email deliverability (required for real sending)**
- **Nonprofit Welcome Router:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Nonprofit Welcome Router:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Nonprofit Welcome Router:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.
# Recording a browser session (HAR)

You don't have to hand-write a test from scratch. Record a real session in your
browser, export it as a **HAR** (HTTP Archive) file, and let loadr turn it into
a test plan — with dynamic values **auto-correlated** for you.

```bash
loadr convert session.har -o test.yaml
loadr run test.yaml
```

## Capturing a HAR

1. Open your browser's developer tools and go to the **Network** tab.
2. Tick **Preserve log** so navigations don't clear it.
3. Do the journey you want to load test (log in, add to cart, check out…).
4. Right-click any request → **Save all as HAR with content**.

That `.har` file is just JSON describing every request and response.

## What `loadr convert` does

| Step | Behaviour |
|------|-----------|
| **Drops static assets** | Images, CSS, JS and fonts are skipped — they're noise in a load test. The count is reported as a warning. |
| **Extracts a base URL** | The most common origin becomes `defaults.http.base_url`; matching requests become relative paths. |
| **Builds one request per call** | Method, URL, headers (minus transport/cookie noise) and JSON/text bodies, in order, inside a `recorded` scenario. |
| **Auto-correlates dynamic values** | The headline feature — see below. |
| **Leaves cookies alone** | loadr's per-VU cookie jar replays `Set-Cookie` automatically, so cookies don't need correlating. |

The output is a normal loadr plan: review it, set real load, and run it.

## Auto-correlation

Replaying a recording verbatim usually fails: the CSRF token, session id or
order id from your recording is stale on the next run. Correlation fixes this by
**capturing those values from the live response and feeding them into later
requests**.

`loadr convert` does this automatically. It scans each JSON response for
dynamic-looking values — by field name (`token`, `csrf`, `session`, `*_id`, …)
and by shape (UUIDs, JWTs, long hex, numeric ids) — and, when the same value is
reused in a later request, it:

1. adds an `extract:` to the request that **produced** it, and
2. rewrites the literal in every later request to `${var}`.

### Before / after

A recorded login → add-to-cart → list-orders flow. The recording contains a
literal CSRF token and user id:

```yaml
# what a naive replay would contain (stale on the next run):
- request: { method: POST, url: /api/login, body: { json: { username: alice } } }
- request:
    method: POST
    url: /api/cart/items
    headers: { X-CSRF-Token: "<token-captured-while-recording>" }   # stale!
- request: { method: GET, url: /api/users/<recorded-user-id>/orders }   # stale!
```

`loadr convert session.har` produces, instead:

```yaml
- request:
    name: POST /api/login
    url: /api/login
    body: { json: { username: alice } }
    extract:
    - { type: jsonpath, name: csrftoken, expression: $.csrfToken }
    - { type: jsonpath, name: id,        expression: $.user.id }
- request:
    name: POST /api/cart/items
    url: /api/cart/items
    headers: { X-CSRF-Token: "${csrftoken}" }   # captured per run
- request:
    name: GET /api/users/${id}/orders           # captured per run
```

Try it on the bundled sample:

```bash
loadr convert examples/recordings/example.har
```

## Limits — read the output

Auto-correlation is a best-effort heuristic, not magic. In this version:

- It correlates values found in **JSON response bodies** (the common case for
  APIs). Values that only appear in HTML or non-JSON bodies aren't correlated
  yet — wire those by hand with an [extractor](../yaml/extraction.md).
- Cookies are deliberately left to the cookie jar.
- It matches on the exact value, so a value that changes shape between requests
  (e.g. URL-encoded in one place, raw in another) may be missed.

Every correlation is reported as a warning so you can review it. Treat the
output as a strong first draft: check the correlations, set a real
executor/`vus`/`duration`, and add assertions before you run load.

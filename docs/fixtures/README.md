# Sanitized fixtures

This directory holds **sanitized** example responses for third-party APIs you
integrate with a vault entry.

Rules for anything placed here:

- It is documentation, **not** a secret. Never include a real API token, key,
  account id, or any credential — not even a truncated one.
- Prefer a real response with sensitive scalar values replaced by
  placeholders (`"…"`, `0`, `null`, or `<redacted>`).
- Keep the **shape** (field names, nesting, types) intact, because the point of
  a fixture is to let a developer or an AI agent code against the real
  structure without touching the vault.
- Do not paste in content that is private to your own account — real account
  ids, expiry timestamps, machine bindings, or anything identifying. Shape is
  what matters here, not values.
- Format follows the source: JSON endpoints use `<api>.json`, RSS and other XML
  endpoints use `<api>-<feed>.xml`.

The API contract itself lives in `docs/<api>.md`. See `AGENTS.md` for why these
belong in the repository rather than in the encrypted vault.
# B2 Native API: Authorization Header Format

## Finding

The Backblaze B2 Native API does **not** use the `Bearer` token scheme for authenticated requests.

After calling `b2_authorize_account`, the returned `authorizationToken` must be sent as the raw value of the `Authorization` header:

```
Authorization: <authorizationToken>
```

It must **not** be prefixed with `Bearer `:

```
Authorization: Bearer <authorizationToken>   ← WRONG — causes bad_auth_token (401)
```

`reqwest`'s `.bearer_auth()` method adds the `Bearer ` prefix automatically, which is incorrect for B2. Use `.header("Authorization", token)` instead.

## Affected calls

All post-auth B2 API calls: `b2_list_buckets`, `b2_create_key`, `b2_delete_key`.

## Symptom

Sharing fails with:

```
B2 bucket lookup failed: B2 API error: b2_list_buckets returned HTTP 401: {"code":"bad_auth_token","message":"","status":401}
```

Even though `b2_authorize_account` succeeds and the key has all required capabilities (`listBuckets`, `writeKeys`, etc.).

## Fix

`src-tauri/src/sharing/b2_api.rs` — replaced `.bearer_auth(&auth.authorization_token)` with `.header("Authorization", &auth.authorization_token)` in all three functions.

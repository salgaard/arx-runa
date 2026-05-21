# The Security Model

This page states what Arx Runa's zero-knowledge design means in concrete terms: who is trusted, who isn't, what each party can and cannot learn, and where each guarantee is implemented. Read this first if you're evaluating whether to trust the system.

## The trust boundary

Arx Runa is designed around a single question: *what must you trust?*

You must trust your own device and your own password. Everything else is untrusted — by design.

| Party | Trusted for | Not trusted for |
|---|---|---|
| Your device | Running the app, holding session keys in RAM | Permanent storage of secrets — keys are zeroed when the session closes |
| Your password (+ USB key) | Deriving the master key | Nothing reaches the cloud without this |
| Your cloud provider | Storing blobs reliably, returning them when asked | Confidentiality — they receive only encrypted ciphertext |
| Arx Runa | Shipping the software | Nothing — Arx Runa operates no server and receives no data from you |
| The network | Delivering bytes | Confidentiality — the rclone transport layer handles TLS; the blobs are encrypted before they enter that channel |

The last entry deserves emphasis: **there is no Arx Runa server**. The app on your device talks directly to your cloud storage provider through rclone. Arx Runa never sees your files, your keys, your vault structure, or even which provider you've chosen. There is no account with Arx Runa, no telemetry, no central relay. This is not a policy — it is an architectural property. Even if Arx Runa wanted to learn about your data, there is no channel through which that could happen.

## What the cloud cannot learn

An adversary with full read access to your cloud storage — a subpoena, a breach, a rogue employee, a court order to your provider — sees this:

| What they see | What it reveals |
|---|---|
| A flat directory of blobs named with random UUIDs | Nothing — names have no relation to file identity |
| All blobs padded to the same fixed size | Nothing — individual file sizes are hidden |
| The number of blobs | A lower bound on vault size (blob count × 4 MiB) — not file count, not individual sizes |
| Upload and download timing | When you are active; upload order is randomised, so blob-to-file mapping cannot be inferred from timing alone |
| `vault-header.json` | That Arx Runa is in use, and which Argon2id parameters were chosen — no key material |
| The encrypted manifest backup | That a manifest backup exists — the content is AEAD-encrypted and unreadable without your master key |
| File names, folder structure, metadata, file content | Nothing — all of this lives inside authenticated ciphertext |

The full cloud layout and threat analysis is in [What the Cloud Sees](cloud-sync.md).

## What a sharing recipient cannot learn

When you share a file, the recipient receives a `.vgshare` package containing a copy of that file's encryption key, sealed to their public key with [HPKE](../guides/glossary.md). They can decrypt the specific file they were sent. They cannot:

- Derive your master key or any of your session keys
- Decrypt any other file in your vault
- See your directory structure or learn which other files you have
- Learn how many other recipients the same file was shared with

The share is cryptographically scoped: one key, one file, one recipient. The mechanism is in [Sharing Files Privately](file-sharing.md).

## What "zero-knowledge" means here

"Zero-knowledge" in Arx Runa's context means that the cloud learns nothing about the contents of your vault — not in the information-theoretic sense of zero-knowledge proofs, but in the practical sense: the encrypted ciphertext reaching the cloud is computationally indistinguishable from random noise to anyone without your key. The guarantee holds even if:

- The cloud provider is actively malicious
- Your cloud provider cooperates fully with an adversary
- The transport layer is intercepted (the blobs are encrypted before they enter it)
- Arx Runa itself is compromised (there is no server to compromise)

This is enforced at multiple levels:

- **Key derivation** — the master key is derived entirely on your device from your password and optional USB key; it never leaves your device ([The Vault](the-vault.md), [Unlocking](unlocking.md))
- **Per-file key isolation** — each file has its own encryption key, wrapped by your key encryption key; the cloud sees only wrapped keys inside encrypted ciphertext ([The Vault](the-vault.md))
- **Authenticated encryption** — every chunk is encrypted with XChaCha20-Poly1305; any modification is detected before decryption proceeds ([How Files Are Encrypted](file-encryption.md))
- **No plaintext on disk** — the decrypt pipeline never writes plaintext to a temp file; the output is a locked memory buffer that zeroes itself on drop ([How Files Are Encrypted](file-encryption.md))
- **Opaque cloud layout** — random UUIDs, uniform blob sizes, randomised upload order ([What the Cloud Sees](cloud-sync.md))

## The honest limits

No system is unconditionally secure, and Arx Runa should not pretend otherwise.

**What the cloud does learn:**
- A lower bound on how much data you store (blob count × 4 MiB). This is inherent to any cloud backup system.
- The timing of your active sessions — when you push or pull, the cloud sees network activity.
- That Arx Runa is being used — the `vault-header.json` format is recognisable.

**What the system cannot protect against:**
- A compromised device — if your device is running malware, session keys in RAM are accessible. Arx Runa protects data at rest and in transit; it is not an endpoint security tool.
- A weak password — Argon2id makes brute-force expensive but not impossible. A short or common password substantially reduces the cost of an offline attack if the vault-header.json is obtained.
- Physical access to a locked-out session — zeroization only happens on clean close. An attacker with a RAM dump during an active session may be able to extract session keys. This is a hardware-level threat outside Arx Runa's scope.

**What sharing receipts reveal:**
If you request a download receipt, the receipt upload is visible to the cloud provider — they see that a blob was written to the `receipts/` prefix of the shared path. They cannot read the receipt content (it is HPKE-sealed to your public key), but they can infer that a download happened. The timing and existence of receipts are metadata, not content. This is opt-in and documented in [Sharing Files Privately](file-sharing.md).

## Reading order

If you want to trace the full trust chain from password to encrypted blob:

1. [The Vault](the-vault.md) — the master key and the key tree
2. [Unlocking: Password and USB Key](unlocking.md) — how the master key is derived
3. [How Files Are Encrypted and Decrypted](file-encryption.md) — per-file keys, chunking, AEAD, in-memory output
4. [What the Cloud Sees](cloud-sync.md) — cloud layout and what an attacker observes
5. [Sharing Files Privately](file-sharing.md) — HPKE share packages and receipts
6. [Recovery: If You Lose Your Key](recovery.md) — the limits of recovery and what it doesn't change about the security model

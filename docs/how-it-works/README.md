# How It Works

These six pages explain what Arx Runa does and why it is trustworthy — not how to build it, and not how to use it. The audience is anyone who wants to understand the security model before deciding whether to trust the system.

The one guarantee that runs through every page: **everything is encrypted before it leaves your device**. The cloud receives opaque blobs. No file names, no directory structure, no metadata, no keys. Arx Runa is designed so that even a fully compromised cloud provider learns nothing about your files.

---

- [The Vault](the-vault.md) — What a vault is, where it lives, and why the master key never touches the cloud.
- [Unlocking: Password and USB Key](unlocking.md) — What happens when you unlock, what Argon2id does, and what the USB key adds.
- [Recovery: If You Lose Your Key](recovery.md) — Recovery phrases, new-device bootstrap, and the honest limits of recovery.
- [How Files Are Encrypted and Decrypted](file-encryption.md) — The full round-trip: EXIF stripping, chunking, padding, per-file keys, authenticated encryption, integrity verification, and reassembly.
- [What the Cloud Sees](cloud-sync.md) — The cloud layout, what an attacker with storage access can and cannot learn, and how multi-destination backup works.
- [Sharing Files Privately](file-sharing.md) — Public-key sharing with HPKE, snapshot semantics, revocation, and the role of key fingerprints.

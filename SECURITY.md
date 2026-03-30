# Security Policy

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

If you discover a security vulnerability in VoidGate, please report it
responsibly:

1. **Email**: [INSERT SECURITY EMAIL] (preferred)
2. **GitHub Security Advisories**: Use the "Report a vulnerability" button in
   the Security tab

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Any suggested fixes (optional)

### What to Expect

- **Acknowledgment**: Within 48 hours
- **Initial assessment**: Within 7 days
- **Resolution timeline**: Depends on severity, typically 30-90 days
- **Credit**: We will credit you in the release notes (unless you prefer anonymity)

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.x.x   | :white_check_mark: |

Once we reach 1.0, we will maintain security updates for the latest minor
version.

## Security Model

VoidGate is a zero-knowledge encrypted storage system. Our security model
assumes:

### In Scope (Protected Against)

- Cloud provider reading file contents
- Cloud provider inferring file sizes (via fixed-size padding)
- Network eavesdropping (all data encrypted client-side)
- Offline brute-force attacks (Argon2id with OWASP-recommended parameters)
- Single-factor compromise (USB key file required alongside password)

### Out of Scope (Not Protected Against)

- Compromised client operating system or kernel
- Cold boot attacks on client RAM
- Malware with root/admin access on client
- Physical access to unlocked client device
- Side-channel attacks on client CPU

For full details, see our [Threat Model](docs/threat-model/).

## Security Best Practices

When contributing to VoidGate:

1. **Never log sensitive data** — keys, passwords, plaintext
2. **Use `zeroize`** — all key material must implement `ZeroizeOnDrop`
3. **Use `secrecy`** — wrap keys in `Secret<T>` to prevent accidental exposure
4. **Include AAD** — all AEAD operations must include authenticated associated data
5. **Use CSPRNG** — nonces must be randomly generated, never sequential
6. **No `unsafe` without justification** — requires `// SAFETY:` comment

## Acknowledgments

We thank the following individuals for responsibly disclosing vulnerabilities:

_No reports yet._

# Contributing to VoidGate

Thank you for your interest in contributing to VoidGate! This document provides
guidelines and instructions for contributing.

## Getting Started

1. **Fork the repository** and clone your fork
2. **Set up the development environment** — see [Development Setup](docs/guides/development.md)
3. **Create a feature branch** from `development`

## Development Workflow

### Branch Naming

Use descriptive branch names with a prefix:

- `feature/` — new features (e.g., `feature/chunk-streaming`)
- `fix/` — bug fixes (e.g., `fix/memory-leak-on-decrypt`)
- `docs/` — documentation changes (e.g., `docs/update-threat-model`)
- `refactor/` — code refactoring (e.g., `refactor/key-derivation`)
- `test/` — test additions or fixes (e.g., `test/aead-round-trip`)

### Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer]
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`

**Examples:**

```
feat(crypto): add XChaCha20-Poly1305 chunk encryption
fix(auth): zero session keys on timeout
docs(architecture): add key derivation tree diagram
test(storage): add BLAKE3 integrity check tests
```

### Pull Request Process

1. **Create a PR** against the `development` branch
2. **Fill out the PR template** completely
3. **Ensure CI passes** — all checks must be green
4. **Request review** — CODEOWNERS will be auto-assigned
5. **Address feedback** — push additional commits as needed
6. **Squash and merge** — maintainers will merge once approved

## Code Style

### Rust

- Run `cargo fmt` before committing
- Run `cargo clippy -- -D warnings` — no warnings allowed
- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- See [`.github/instructions/rust.instructions.md`](.github/instructions/rust.instructions.md) for project-specific rules

### Documentation

- Every public item needs a doc comment (`///`)
- Use complete sentences with proper punctuation
- Include examples where helpful

### Security-Critical Code

Code in `crypto/`, `auth/`, or `storage/` has additional requirements:

- Sensitive buffers must use `zeroize` and `secrecy`
- AEAD operations must include proper AAD
- Nonces must be generated via CSPRNG
- No `unwrap()` or `expect()` in production code
- Requires explicit security review before merge

## Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test module
cargo test crypto::

# Run with coverage (requires cargo-llvm-cov)
cargo llvm-cov --html
```

## Reporting Issues

- **Bugs** — use the [Bug Report template](https://github.com/Chorizzio/void-gate/issues/new?template=bug_report.md)
- **Features** — use the [Feature Request template](https://github.com/Chorizzio/void-gate/issues/new?template=feature_request.md)
- **Security vulnerabilities** — see [SECURITY.md](SECURITY.md)

## Questions?

Open a [Discussion](https://github.com/Chorizzio/void-gate/discussions) for
questions, ideas, or general conversation.

## License

By contributing, you agree that your contributions will be licensed under the
same license as the project (license TBD).

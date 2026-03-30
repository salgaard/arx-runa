## Description

<!-- Describe your changes in detail -->

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] Documentation update
- [ ] Refactoring (no functional changes)
- [ ] CI/CD or tooling changes

## Related Issues

<!-- Link to related issues: Fixes #123, Closes #456 -->

## Testing

- [ ] I have added tests that prove my fix/feature works
- [ ] All existing tests pass locally (`cargo test`)
- [ ] I have run `cargo clippy` with no warnings
- [ ] I have run `cargo fmt` to format my code

## Security Checklist

<!-- Complete if your changes touch crypto/, auth/, or storage/ -->

- [ ] No secrets or key material logged or exposed
- [ ] Sensitive buffers use `zeroize` and `secrecy`
- [ ] AEAD operations include proper AAD
- [ ] Nonces are generated via CSPRNG (never sequential)

## Reviewer Notes

<!-- Anything specific reviewers should look at or know about? -->

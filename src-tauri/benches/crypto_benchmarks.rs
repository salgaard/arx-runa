//! Criterion benchmarks for Arx Runa cryptographic primitives.
//!
//! Measures Argon2id key-derivation latency and XChaCha20-Poly1305
//! chunk encrypt/decrypt throughput under production parameters.
//! Results are reported in Bilag C of the bachelor report.

use argon2::{Algorithm, Argon2, Params, Version};
use arx_runa_tauri_lib::crypto::{
    ChunkIndex, FileId, compute_checksum, decrypt_chunk, encrypt_chunk, generate_file_key,
    verify_checksum,
};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use zeroize::Zeroizing;

/// Argon2Params::DEFAULT — must stay in sync with `src-tauri/src/auth/kdf.rs`.
const MEMORY_COST_KIB: u32 = 65_536;
const TIME_COST: u32 = 3;
const PARALLELISM: u32 = 4;
const MASTER_KEY_LEN: usize = 32;

/// Measures Argon2id master-key derivation with production parameters.
///
/// This is the dominant cost in vault unlock. The benchmark uses the same
/// `m=65536 KiB, t=3, p=4` parameters as `Argon2Params::DEFAULT` (RFC 9106 §4).
fn bench_argon2id(c: &mut Criterion) {
    let password = b"correct horse battery staple";
    let salt = [0xABu8; 32];
    let mut output = [0u8; MASTER_KEY_LEN];

    let params = Params::new(
        MEMORY_COST_KIB,
        TIME_COST,
        PARALLELISM,
        Some(MASTER_KEY_LEN),
    )
    .unwrap();
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    c.bench_function("argon2id_derive_master_key", |b| {
        b.iter(|| {
            argon2
                .hash_password_into(password, &salt, &mut output)
                .unwrap();
        });
    });
}

/// Measures XChaCha20-Poly1305 chunk encrypt and decrypt throughput.
///
/// Two chunk sizes are benchmarked:
/// - 512 KiB: smaller vaults / slow-connection optimised setting
/// - 4 MiB: default production chunk size
fn bench_chunk_crypto(c: &mut Criterion) {
    let file_key = generate_file_key();
    let file_id = FileId::new([0x11u8; 16]);
    let chunk_index = ChunkIndex::new(0);

    let mut group = c.benchmark_group("chunk_crypto");

    for &size_bytes in &[512 * 1024usize, 4 * 1024 * 1024] {
        let label = if size_bytes < 1024 * 1024 {
            format!("{}KiB", size_bytes / 1024)
        } else {
            format!("{}MiB", size_bytes / (1024 * 1024))
        };
        let plaintext = vec![0xBBu8; size_bytes];

        group.throughput(Throughput::Bytes(size_bytes as u64));

        group.bench_with_input(BenchmarkId::new("encrypt", &label), &size_bytes, |b, _| {
            b.iter(|| {
                encrypt_chunk(
                    Zeroizing::new(plaintext.clone()),
                    &file_key,
                    &file_id,
                    chunk_index,
                )
                .unwrap()
            });
        });

        let blob = encrypt_chunk(
            Zeroizing::new(plaintext.clone()),
            &file_key,
            &file_id,
            chunk_index,
        )
        .unwrap();
        let checksum = compute_checksum(&blob);

        group.bench_with_input(BenchmarkId::new("decrypt", &label), &size_bytes, |b, _| {
            b.iter(|| {
                let verified = verify_checksum(blob.clone(), &checksum).unwrap();
                decrypt_chunk(verified, &file_key, &file_id, chunk_index).unwrap()
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_argon2id, bench_chunk_crypto);
criterion_main!(benches);

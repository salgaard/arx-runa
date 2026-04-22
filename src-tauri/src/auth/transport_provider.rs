//! TransportProvider trait for dependency injection of cloud transport layer.
//!
//! Decouples auth ceremonies from concrete CloudTransport implementations,
//! enabling offline-first testing and alternative transport strategies.

pub use crate::storage::cloud::CloudTransport as TransportProvider;

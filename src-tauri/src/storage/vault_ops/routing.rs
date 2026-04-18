/// Route decision for upload chunk handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteDecision {
    /// Upload goes through immediate standalone chunking.
    Immediate,
    /// Upload should route through epoch buffering.
    EpochBuffer,
}

/// Decides upload route from file size, chunk size, and epoch-buffer flag.
pub fn decide(file_size: u64, chunk_size_bytes: u64, epoch_enabled: bool) -> RouteDecision {
    if epoch_enabled && file_size < chunk_size_bytes {
        RouteDecision::EpochBuffer
    } else {
        RouteDecision::Immediate
    }
}

#[cfg(test)]
mod tests {
    use super::{RouteDecision, decide};

    /// Verifies epoch-disabled small files route immediate.
    #[test]
    fn test_decide_epoch_disabled_small_file_returns_immediate() {
        assert_eq!(decide(1, 131_072, false), RouteDecision::Immediate);
    }

    /// Verifies epoch-disabled large files route immediate.
    #[test]
    fn test_decide_epoch_disabled_large_file_returns_immediate() {
        assert_eq!(decide(10_000_000, 131_072, false), RouteDecision::Immediate);
    }

    /// Verifies epoch-enabled small files route to epoch buffer.
    #[test]
    fn test_decide_epoch_enabled_small_file_returns_epoch_buffer() {
        assert_eq!(decide(1, 131_072, true), RouteDecision::EpochBuffer);
    }

    /// Verifies epoch-enabled boundary equal to chunk size routes immediate.
    #[test]
    fn test_decide_epoch_enabled_exactly_chunk_size_returns_immediate() {
        assert_eq!(decide(131_072, 131_072, true), RouteDecision::Immediate);
    }

    /// Verifies epoch-enabled larger files route immediate.
    #[test]
    fn test_decide_epoch_enabled_large_file_returns_immediate() {
        assert_eq!(decide(262_144, 131_072, true), RouteDecision::Immediate);
    }

    /// Verifies zero-byte epoch-enabled files route to epoch buffer.
    #[test]
    fn test_decide_zero_byte_epoch_enabled_returns_epoch_buffer() {
        assert_eq!(decide(0, 131_072, true), RouteDecision::EpochBuffer);
    }

    /// Verifies zero-byte epoch-disabled files route immediate.
    #[test]
    fn test_decide_zero_byte_epoch_disabled_returns_immediate() {
        assert_eq!(decide(0, 131_072, false), RouteDecision::Immediate);
    }
}

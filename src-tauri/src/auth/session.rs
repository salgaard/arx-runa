//! Session-key container backed by memory-locked heap buffers.
//!
//! Owns all derived vault-level keys for the duration of an authenticated
//! session. Keys are locked on construction and zeroed on drop.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::{RwLock, broadcast, oneshot, watch};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::auth::KeySource;
use crate::auth::config;
use crate::auth::error::{AuthenticationError, KeySourceError};
use crate::auth::kdf::{Argon2Params, derive_master_key_into};
use crate::crypto::hkdf::{
    HKDF_INFO_KEY_ENCRYPTION, HKDF_INFO_MANIFEST_BACKUP, HKDF_INFO_SQLCIPHER, expand_vault_key_into,
};
use crate::memory::SecureBytes;

/// Holds all derived keys for the duration of an authenticated session.
pub(crate) struct SessionKeys {
    /// Key-encryption key used to wrap file keys.
    pub(crate) key_encryption_key: SecureBytes<32>,
    /// SQLCipher key used for the metadata database.
    pub(crate) sqlcipher_key: SecureBytes<32>,
    /// Manifest key used for manifest backup encryption.
    pub(crate) manifest_key: SecureBytes<32>,
}

impl SessionKeys {
    /// Derives `master_key` via Argon2id and expands it into three locked
    /// vault-level keys.
    pub(crate) fn derive(
        password_utf8_bytes: &[u8],
        key_file_bytes: Option<&[u8; 32]>,
        salt: &[u8; 32],
        params: &Argon2Params,
    ) -> Result<Self, AuthenticationError> {
        let mut key_encryption_key = SecureBytes::<32>::new()?;
        let mut sqlcipher_key = SecureBytes::<32>::new()?;
        let mut manifest_key = SecureBytes::<32>::new()?;

        let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            password_utf8_bytes,
            key_file_bytes,
            salt,
            params,
            &mut master_key,
        )?;

        expand_vault_key_into(
            &master_key,
            HKDF_INFO_KEY_ENCRYPTION,
            key_encryption_key.as_mut(),
        )
        .map_err(|_| AuthenticationError::InvalidCredentials)?;
        expand_vault_key_into(&master_key, HKDF_INFO_SQLCIPHER, sqlcipher_key.as_mut())
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        expand_vault_key_into(
            &master_key,
            HKDF_INFO_MANIFEST_BACKUP,
            manifest_key.as_mut(),
        )
        .map_err(|_| AuthenticationError::InvalidCredentials)?;

        drop(master_key);

        Ok(Self {
            key_encryption_key,
            sqlcipher_key,
            manifest_key,
        })
    }
}

const PRE_WARNING_SECONDS: u64 = 60;
const BROADCAST_CHANNEL_CAPACITY: usize = 16;

/// Session lifecycle state for authentication and timeout transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// No session has been established yet.
    NoSession,
    /// Session keys are currently resident in locked memory.
    Active,
    /// A prior session has been locked and keys were dropped.
    Expired,
}

/// Event emitted by the session lifecycle manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// Emitted shortly before timeout-triggered lock.
    TimeoutWarning { seconds_remaining: u64 },
    /// Emitted after a lock operation completes.
    Locked,
}

type SharedSession = Arc<RwLock<Option<SessionKeys>>>;

/// Handle to an in-flight timer task.
struct TimerHandle {
    cancel: oneshot::Sender<()>,
    join: JoinHandle<()>,
}

/// Captures resources consumed by the timeout task.
struct TimerContext {
    event_sender: broadcast::Sender<SessionEvent>,
    session: SharedSession,
    lifecycle: Arc<RwLock<LifecycleState>>,
    counter: watch::Receiver<u32>,
    operation_gate_closed: Arc<AtomicBool>,
}

/// Owns session lifecycle transitions, timeout management, and operation gating.
pub struct SessionManager {
    session: SharedSession,
    lifecycle: Arc<RwLock<LifecycleState>>,
    timeout: Duration,
    pre_warning_duration: Duration,
    timer: Arc<tokio::sync::Mutex<Option<TimerHandle>>>,
    operation_counter_sender: watch::Sender<u32>,
    operation_counter_receiver: watch::Receiver<u32>,
    operation_gate_closed: Arc<AtomicBool>,
    event_sender: broadcast::Sender<SessionEvent>,
}

/// RAII guard that decrements the operation counter on drop.
#[must_use = "dropping the guard decrements the operation counter"]
pub struct OperationGuard {
    sender: Option<watch::Sender<u32>>,
}

impl Drop for OperationGuard {
    /// Decrements the operation counter when an operation scope exits.
    fn drop(&mut self) {
        if let Some(sender) = self.sender.as_ref() {
            sender.send_modify(|count| {
                *count = count.saturating_sub(1);
            });
        }
    }
}

impl SessionManager {
    /// Constructs a manager using timeout values from local config.
    pub fn from_config() -> Self {
        Self::with_timeout(config::load_session_timeout())
    }

    /// Constructs a manager with an explicit timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        let (operation_counter_sender, operation_counter_receiver) = watch::channel(0u32);
        let (event_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        Self {
            session: Arc::new(RwLock::new(None)),
            lifecycle: Arc::new(RwLock::new(LifecycleState::NoSession)),
            timeout,
            pre_warning_duration: Duration::from_secs(PRE_WARNING_SECONDS),
            timer: Arc::new(tokio::sync::Mutex::new(None)),
            operation_counter_sender,
            operation_counter_receiver,
            operation_gate_closed: Arc::new(AtomicBool::new(true)),
            event_sender,
        }
    }

    /// Constructs a manager with explicit timeout and warning durations.
    #[cfg(test)]
    fn with_timeout_and_warning(timeout: Duration, pre_warning_duration: Duration) -> Self {
        let (operation_counter_sender, operation_counter_receiver) = watch::channel(0u32);
        let (event_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        Self {
            session: Arc::new(RwLock::new(None)),
            lifecycle: Arc::new(RwLock::new(LifecycleState::NoSession)),
            timeout,
            pre_warning_duration,
            timer: Arc::new(tokio::sync::Mutex::new(None)),
            operation_counter_sender,
            operation_counter_receiver,
            operation_gate_closed: Arc::new(AtomicBool::new(true)),
            event_sender,
        }
    }

    /// Returns the current lifecycle state.
    pub async fn state(&self) -> LifecycleState {
        *self.lifecycle.read().await
    }

    /// Returns a broadcast receiver for session events.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_sender.subscribe()
    }

    /// Authenticates and installs derived keys into the active session.
    pub async fn authenticate(
        &self,
        password_utf8_bytes: &[u8],
        key_source: Option<&(dyn KeySource + Send + Sync)>,
        salt: &[u8; 32],
        params: &Argon2Params,
    ) -> Result<(), AuthenticationError> {
        if self.state().await == LifecycleState::Active {
            return Err(AuthenticationError::SessionAlreadyActive);
        }

        let key_file_bytes = match key_source {
            Some(source) => match source.read_key() {
                Ok(bytes) => Some(bytes),
                Err(KeySourceError::NotFound) | Err(KeySourceError::InvalidSize { .. }) => {
                    return Err(AuthenticationError::KeyFileNotFound);
                }
                Err(KeySourceError::IoFailed(error)) => {
                    tracing::warn!(?error, "key source IO failed during authenticate");
                    return Err(AuthenticationError::InvalidCredentials);
                }
            },
            None => None,
        };

        let password_owned = Zeroizing::new(password_utf8_bytes.to_vec());
        let key_file_owned = key_file_bytes;
        let salt_owned = *salt;
        let params_owned = *params;

        let derived = tokio::task::spawn_blocking(move || {
            let key_file_ref = key_file_owned.as_deref();
            SessionKeys::derive(&password_owned, key_file_ref, &salt_owned, &params_owned)
        })
        .await
        .map_err(|join_error| {
            tracing::error!(
                ?join_error,
                "spawn_blocking for SessionKeys::derive panicked"
            );
            AuthenticationError::InvalidCredentials
        })??;
        if !Self::derived_keys_are_initialized(&derived) {
            tracing::error!("derived session keys contain an all-zero key buffer");
            return Err(AuthenticationError::InvalidCredentials);
        }

        // TODO(phase-3.1): open SQLCipher DB with derived.sqlcipher_key here.
        {
            let mut session_guard = self.session.write().await;
            *session_guard = Some(derived);
        }
        {
            let mut lifecycle_guard = self.lifecycle.write().await;
            *lifecycle_guard = LifecycleState::Active;
        }
        self.operation_gate_closed.store(false, Ordering::SeqCst);

        self.restart_timer().await;
        // TODO(phase-2.4/6.1): add per-vault exponential backoff on InvalidCredentials.
        Ok(())
    }

    /// Locks the session, waiting for active operations before key drop.
    pub async fn lock(&self) {
        if self.state().await != LifecycleState::Active {
            return;
        }

        self.operation_gate_closed.store(true, Ordering::SeqCst);
        self.cancel_timer().await;

        let mut waiter = self.operation_counter_receiver.clone();
        if let Err(error) = waiter.wait_for(|count| *count == 0).await {
            tracing::error!(?error, "operation counter channel closed during lock");
        }

        // TODO(phase-3.1): close SQLCipher connection here before zeroizing.
        // TODO(phase-4): overwrite and unlink temp rclone.conf here before zeroizing.
        {
            let mut session_guard = self.session.write().await;
            *session_guard = None;
        }
        {
            let mut lifecycle_guard = self.lifecycle.write().await;
            if *lifecycle_guard != LifecycleState::Active {
                return;
            }
            *lifecycle_guard = LifecycleState::Expired;
        }

        let _ = self.event_sender.send(SessionEvent::Locked);
    }

    /// Restarts inactivity timeout when a session-scoped command is executed.
    pub async fn reset_timer(&self) {
        if self.state().await != LifecycleState::Active {
            return;
        }

        self.restart_timer().await;
    }

    /// Starts an operation scope that blocks timeout lock until dropped.
    pub fn begin_operation(&self) -> OperationGuard {
        if self.operation_gate_closed.load(Ordering::SeqCst) {
            return OperationGuard { sender: None };
        }

        self.operation_counter_sender
            .send_modify(|count| *count = count.saturating_add(1));
        if self.operation_gate_closed.load(Ordering::SeqCst) {
            self.operation_counter_sender
                .send_modify(|count| *count = count.saturating_sub(1));
            return OperationGuard { sender: None };
        }

        OperationGuard {
            sender: Some(self.operation_counter_sender.clone()),
        }
    }

    /// Returns `true` when all derived key buffers contain at least one non-zero byte.
    fn derived_keys_are_initialized(derived: &SessionKeys) -> bool {
        [
            derived.key_encryption_key.expose(),
            derived.sqlcipher_key.expose(),
            derived.manifest_key.expose(),
        ]
        .iter()
        .all(|buffer| buffer.iter().any(|byte| *byte != 0))
    }

    /// Cancels the currently active timeout task, if present.
    async fn cancel_timer(&self) {
        let mut timer_slot = self.timer.lock().await;
        if let Some(handle) = timer_slot.take() {
            let _ = handle.cancel.send(());
            handle.join.abort();
        }
    }

    /// Cancels and respawns the timeout task.
    async fn restart_timer(&self) {
        self.cancel_timer().await;

        let (cancel_tx, cancel_rx) = oneshot::channel();
        let timeout = self.timeout;
        let pre_warning_duration = self.pre_warning_duration;
        let context = TimerContext {
            event_sender: self.event_sender.clone(),
            session: Arc::clone(&self.session),
            lifecycle: Arc::clone(&self.lifecycle),
            counter: self.operation_counter_receiver.clone(),
            operation_gate_closed: Arc::clone(&self.operation_gate_closed),
        };

        let join = tokio::spawn(Self::run_timer(
            timeout,
            pre_warning_duration,
            cancel_rx,
            context,
        ));

        let mut timer_slot = self.timer.lock().await;
        *timer_slot = Some(TimerHandle {
            cancel: cancel_tx,
            join,
        });
    }

    /// Executes timeout flow and emits warning and lock events.
    async fn run_timer(
        timeout: Duration,
        pre_warning_duration: Duration,
        cancel: oneshot::Receiver<()>,
        mut context: TimerContext,
    ) {
        tokio::pin!(cancel);

        if pre_warning_duration > Duration::ZERO && timeout > pre_warning_duration {
            let pre_warning_sleep = timeout - pre_warning_duration;
            tokio::select! {
                _ = tokio::time::sleep(pre_warning_sleep) => {}
                _ = &mut cancel => return,
            }
            let _ = context.event_sender.send(SessionEvent::TimeoutWarning {
                seconds_remaining: PRE_WARNING_SECONDS,
            });
            tokio::select! {
                _ = tokio::time::sleep(pre_warning_duration) => {}
                _ = &mut cancel => return,
            }
        } else {
            tokio::select! {
                _ = tokio::time::sleep(timeout) => {}
                _ = &mut cancel => return,
            }
        }

        context.operation_gate_closed.store(true, Ordering::SeqCst);
        if let Err(error) = context.counter.wait_for(|count| *count == 0).await {
            tracing::error!(
                ?error,
                "operation counter channel closed before timeout lock"
            );
            return;
        }

        {
            let mut session_guard = context.session.write().await;
            *session_guard = None;
        }
        {
            let mut lifecycle_guard = context.lifecycle.write().await;
            if *lifecycle_guard != LifecycleState::Active {
                return;
            }
            *lifecycle_guard = LifecycleState::Expired;
        }

        let _ = context.event_sender.send(SessionEvent::Locked);
    }
}

#[cfg(test)]
mod tests {
    use super::{Argon2Params, SessionKeys};
    use crate::memory::platform::set_force_lock_failure;

    const TEST_PARAMS: Argon2Params = Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    };
    const TEST_SALT: [u8; 32] = [0x44u8; 32];

    struct ForceLockFailureGuard;

    impl ForceLockFailureGuard {
        fn new() -> Self {
            set_force_lock_failure(true);
            Self
        }
    }

    impl Drop for ForceLockFailureGuard {
        fn drop(&mut self) {
            set_force_lock_failure(false);
        }
    }

    #[test]
    fn test_session_keys_derive_tier1_produces_three_distinct_keys() {
        let keys = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS)
            .expect("derive must succeed");
        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_tier2_produces_three_distinct_keys() {
        let key_file = [0x77u8; 32];
        let keys = SessionKeys::derive(b"password", Some(&key_file), &TEST_SALT, &TEST_PARAMS)
            .expect("derive must succeed");
        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_is_deterministic_for_same_inputs() {
        let first = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let second = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_eq!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
        assert_eq!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_eq!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_different_passwords_produce_different_key_encryption_keys() {
        let first = SessionKeys::derive(b"password-a", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let second = SessionKeys::derive(b"password-b", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_ne!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_derive_different_key_files_produce_different_key_encryption_keys() {
        let first = SessionKeys::derive(b"password", Some(&[0x01u8; 32]), &TEST_SALT, &TEST_PARAMS)
            .unwrap();
        let second =
            SessionKeys::derive(b"password", Some(&[0x02u8; 32]), &TEST_SALT, &TEST_PARAMS)
                .unwrap();
        assert_ne!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_tier1_and_tier2_produce_different_key_encryption_keys() {
        let key_file = [0x88u8; 32];
        let tier_one = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let tier_two =
            SessionKeys::derive(b"password", Some(&key_file), &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_ne!(
            tier_one.key_encryption_key.expose(),
            tier_two.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_derive_returns_memory_lock_failed_when_lock_is_forced_to_fail() {
        let _guard = ForceLockFailureGuard::new();
        let result = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS);
        let error = match result {
            Ok(_) => panic!("forced lock failure must propagate"),
            Err(error) => error,
        };
        let crate::auth::error::AuthenticationError::MemoryLockFailed(message) = error else {
            panic!("expected MemoryLockFailed variant, got {error:?}");
        };
        assert_eq!(message, expected_platform_failure_message());
    }

    #[test]
    fn test_session_keys_derive_with_non_ascii_utf8_password_succeeds_for_tier1_and_tier2() {
        let password = "påssw🔐rd漢字";
        let key_file = [0x5Au8; 32];

        let tier_one =
            SessionKeys::derive(password.as_bytes(), None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let tier_two = SessionKeys::derive(
            password.as_bytes(),
            Some(&key_file),
            &TEST_SALT,
            &TEST_PARAMS,
        )
        .unwrap();

        assert_ne!(
            tier_one.key_encryption_key.expose(),
            tier_two.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_derive_with_empty_password_succeeds_for_tier1_and_tier2() {
        let key_file = [0xA5u8; 32];

        let tier_one = SessionKeys::derive(b"", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let tier_two = SessionKeys::derive(b"", Some(&key_file), &TEST_SALT, &TEST_PARAMS).unwrap();

        assert_ne!(
            tier_one.key_encryption_key.expose(),
            tier_two.key_encryption_key.expose()
        );
    }

    #[cfg(target_os = "windows")]
    fn expected_platform_failure_message() -> String {
        String::from(
            "Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa.",
        )
    }

    #[cfg(target_os = "linux")]
    fn expected_platform_failure_message() -> String {
        String::from(
            "Cannot lock memory. Increase the memory lock limit: `ulimit -l unlimited` or edit `/etc/security/limits.conf`.",
        )
    }

    #[cfg(target_os = "macos")]
    fn expected_platform_failure_message() -> String {
        String::from(
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
        )
    }
}

#[cfg(test)]
mod session_manager_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::broadcast;
    use tokio::time::{sleep, timeout};
    use zeroize::Zeroizing;

    use super::{Argon2Params, LifecycleState, PRE_WARNING_SECONDS, SessionEvent, SessionManager};
    use crate::auth::{AuthenticationError, KeySource, KeySourceError, MockKeySource};
    use crate::memory::platform::{clear_last_unlock_snapshot, take_last_unlock_snapshot};

    const TEST_PARAMS: Argon2Params = Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    };
    const TEST_SALT: [u8; 32] = [0x44u8; 32];

    struct NotFoundKeySource;
    struct InvalidSizeKeySource;
    struct IoFailingKeySource;

    impl KeySource for NotFoundKeySource {
        fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError> {
            Err(KeySourceError::NotFound)
        }
    }

    impl KeySource for InvalidSizeKeySource {
        fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError> {
            Err(KeySourceError::InvalidSize { actual: 31 })
        }
    }

    impl KeySource for IoFailingKeySource {
        fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError> {
            Err(KeySourceError::IoFailed(std::io::Error::other(
                "key-source io failure",
            )))
        }
    }

    async fn authenticate_tier1(manager: &SessionManager) {
        manager
            .authenticate(b"password", None, &TEST_SALT, &TEST_PARAMS)
            .await
            .expect("tier1 authentication should succeed");
    }

    async fn authenticate_tier2(manager: &SessionManager) {
        let key_source = MockKeySource::new([0x55u8; 32]);
        manager
            .authenticate(b"password", Some(&key_source), &TEST_SALT, &TEST_PARAMS)
            .await
            .expect("tier2 authentication should succeed");
    }

    async fn wait_for_locked_event(receiver: &mut broadcast::Receiver<SessionEvent>) {
        timeout(Duration::from_millis(700), async {
            loop {
                if matches!(
                    receiver
                        .recv()
                        .await
                        .expect("event channel should stay open"),
                    SessionEvent::Locked
                ) {
                    return;
                }
            }
        })
        .await
        .expect("Locked event should arrive before timeout");
    }

    #[tokio::test]
    async fn test_session_manager_new_starts_in_no_session_state() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));

        assert_eq!(manager.state().await, LifecycleState::NoSession);
    }

    #[tokio::test]
    async fn test_authenticate_tier1_transitions_no_session_to_active() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));

        authenticate_tier1(&manager).await;

        assert_eq!(manager.state().await, LifecycleState::Active);
    }

    #[tokio::test]
    async fn test_authenticate_tier2_transitions_no_session_to_active_with_mock_key_source() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));

        authenticate_tier2(&manager).await;

        assert_eq!(manager.state().await, LifecycleState::Active);
    }

    #[tokio::test]
    async fn test_authenticate_rejects_when_state_is_active() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        authenticate_tier1(&manager).await;

        let error = manager
            .authenticate(b"password", None, &TEST_SALT, &TEST_PARAMS)
            .await
            .expect_err("re-authentication while active must fail");

        assert!(matches!(error, AuthenticationError::SessionAlreadyActive));
    }

    #[tokio::test]
    async fn test_authenticate_returns_key_file_not_found_when_key_source_reports_not_found() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        let source = NotFoundKeySource;

        let error = manager
            .authenticate(b"password", Some(&source), &TEST_SALT, &TEST_PARAMS)
            .await
            .expect_err("missing key source must fail authentication");

        assert!(matches!(error, AuthenticationError::KeyFileNotFound));
    }

    #[tokio::test]
    async fn test_authenticate_returns_key_file_not_found_when_key_source_reports_invalid_size() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        let source = InvalidSizeKeySource;

        let error = manager
            .authenticate(b"password", Some(&source), &TEST_SALT, &TEST_PARAMS)
            .await
            .expect_err("invalid-size key source must fail authentication");

        assert!(matches!(error, AuthenticationError::KeyFileNotFound));
    }

    #[tokio::test]
    async fn test_authenticate_returns_invalid_credentials_when_key_source_io_fails() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        let source = IoFailingKeySource;

        let error = manager
            .authenticate(b"password", Some(&source), &TEST_SALT, &TEST_PARAMS)
            .await
            .expect_err("io-failing key source must fail authentication");

        assert!(matches!(error, AuthenticationError::InvalidCredentials));
    }

    #[tokio::test]
    async fn test_lock_transitions_active_to_expired() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        authenticate_tier1(&manager).await;

        manager.lock().await;

        assert_eq!(manager.state().await, LifecycleState::Expired);
    }

    #[tokio::test]
    async fn test_lock_is_idempotent_when_state_is_no_session() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        let mut receiver = manager.subscribe();

        manager.lock().await;

        assert_eq!(manager.state().await, LifecycleState::NoSession);
        assert!(
            timeout(Duration::from_millis(80), receiver.recv())
                .await
                .is_err(),
            "NoSession lock should not emit events"
        );
    }

    #[tokio::test]
    async fn test_lock_is_idempotent_when_state_is_expired() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        authenticate_tier1(&manager).await;
        manager.lock().await;
        let mut receiver = manager.subscribe();

        manager.lock().await;

        assert_eq!(manager.state().await, LifecycleState::Expired);
        assert!(
            timeout(Duration::from_millis(80), receiver.recv())
                .await
                .is_err(),
            "Expired lock should not emit additional events"
        );
    }

    #[tokio::test]
    async fn test_timeout_fires_after_configured_duration_and_transitions_to_expired() {
        let manager = SessionManager::with_timeout_and_warning(
            Duration::from_millis(120),
            Duration::from_millis(60),
        );
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;

        wait_for_locked_event(&mut receiver).await;

        assert_eq!(manager.state().await, LifecycleState::Expired);
    }

    #[tokio::test]
    async fn test_reset_timer_extends_timeout_when_called_before_deadline() {
        let manager = SessionManager::with_timeout_and_warning(
            Duration::from_millis(240),
            Duration::from_millis(80),
        );
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;
        sleep(Duration::from_millis(160)).await;
        manager.reset_timer().await;
        let locked_before_old_deadline = timeout(Duration::from_millis(120), async {
            loop {
                if matches!(
                    receiver
                        .recv()
                        .await
                        .expect("event channel should stay open"),
                    SessionEvent::Locked
                ) {
                    return;
                }
            }
        })
        .await
        .is_ok();
        assert!(
            !locked_before_old_deadline,
            "reset timer should prevent lock at the old deadline"
        );
        wait_for_locked_event(&mut receiver).await;
        assert_eq!(manager.state().await, LifecycleState::Expired);
    }

    #[tokio::test]
    async fn test_reset_timer_is_noop_when_state_is_no_session() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));

        manager.reset_timer().await;

        assert_eq!(manager.state().await, LifecycleState::NoSession);
    }

    #[tokio::test]
    async fn test_operation_counter_delays_lock_until_guard_dropped() {
        let manager = Arc::new(SessionManager::with_timeout(Duration::from_secs(5)));
        authenticate_tier1(&manager).await;
        let operation_guard = manager.begin_operation();
        let manager_for_lock = Arc::clone(&manager);
        let lock_task = tokio::spawn(async move {
            manager_for_lock.lock().await;
        });

        sleep(Duration::from_millis(80)).await;
        assert_eq!(manager.state().await, LifecycleState::Active);

        drop(operation_guard);
        timeout(Duration::from_millis(500), lock_task)
            .await
            .expect("lock task should complete after operation guard drop")
            .expect("lock task must not panic");
        assert_eq!(manager.state().await, LifecycleState::Expired);
    }

    #[tokio::test]
    async fn test_operation_counter_delays_timeout_until_guard_dropped() {
        let manager = SessionManager::with_timeout_and_warning(
            Duration::from_millis(100),
            Duration::from_millis(30),
        );
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;
        let operation_guard = manager.begin_operation();

        sleep(Duration::from_millis(180)).await;
        assert_eq!(manager.state().await, LifecycleState::Active);

        drop(operation_guard);
        wait_for_locked_event(&mut receiver).await;
        assert_eq!(manager.state().await, LifecycleState::Expired);
    }

    #[tokio::test]
    async fn test_operation_counter_drops_guard_on_panic() {
        let manager = Arc::new(SessionManager::with_timeout(Duration::from_secs(5)));
        authenticate_tier1(&manager).await;

        let manager_for_panic = Arc::clone(&manager);
        let panic_task = tokio::spawn(async move {
            let _guard = manager_for_panic.begin_operation();
            panic!("intentional panic while operation guard is in scope");
        });
        assert!(panic_task.await.is_err(), "task must panic");
        assert_eq!(*manager.operation_counter_receiver.borrow(), 0);

        timeout(Duration::from_millis(500), manager.lock())
            .await
            .expect("lock should not block after panic");
        assert_eq!(manager.state().await, LifecycleState::Expired);
    }

    #[tokio::test]
    async fn test_session_manager_state_transition_no_session_active_expired_active_succeeds() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        assert_eq!(manager.state().await, LifecycleState::NoSession);

        authenticate_tier1(&manager).await;
        assert_eq!(manager.state().await, LifecycleState::Active);

        manager.lock().await;
        assert_eq!(manager.state().await, LifecycleState::Expired);

        authenticate_tier1(&manager).await;
        assert_eq!(manager.state().await, LifecycleState::Active);
    }

    #[tokio::test]
    async fn test_re_authentication_from_expired_transitions_to_active() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        authenticate_tier1(&manager).await;
        manager.lock().await;
        assert_eq!(manager.state().await, LifecycleState::Expired);

        authenticate_tier1(&manager).await;

        assert_eq!(manager.state().await, LifecycleState::Active);
    }

    #[tokio::test]
    async fn test_session_event_timeout_warning_emitted_before_expiry() {
        let manager = SessionManager::with_timeout_and_warning(
            Duration::from_millis(200),
            Duration::from_millis(100),
        );
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;

        let warning = timeout(Duration::from_millis(350), receiver.recv())
            .await
            .expect("timeout warning should be emitted")
            .expect("event channel should stay open");
        assert!(matches!(
            warning,
            SessionEvent::TimeoutWarning {
                seconds_remaining: PRE_WARNING_SECONDS
            }
        ));
        wait_for_locked_event(&mut receiver).await;
    }

    #[tokio::test]
    async fn test_session_event_locked_emitted_on_manual_lock() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;

        manager.lock().await;

        let event = timeout(Duration::from_millis(150), receiver.recv())
            .await
            .expect("Locked event should be emitted on manual lock")
            .expect("event channel should stay open");
        assert!(matches!(event, SessionEvent::Locked));
    }

    #[tokio::test]
    async fn test_session_event_locked_emitted_on_timeout_expiry() {
        let manager = SessionManager::with_timeout(Duration::from_millis(120));
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;

        let event = timeout(Duration::from_millis(350), receiver.recv())
            .await
            .expect("Locked event should be emitted after timeout")
            .expect("event channel should stay open");
        assert!(matches!(event, SessionEvent::Locked));
    }

    #[tokio::test]
    async fn test_session_keys_buffers_are_zeroed_after_lock() {
        let manager = SessionManager::with_timeout(Duration::from_secs(5));
        clear_last_unlock_snapshot();
        authenticate_tier1(&manager).await;

        manager.lock().await;

        let snapshot = take_last_unlock_snapshot().expect("unlock snapshot should be captured");
        assert_eq!(snapshot, vec![0u8; 32]);
    }

    #[tokio::test]
    async fn test_session_manager_manual_lock_during_pending_timeout_emits_single_locked_event() {
        let manager = Arc::new(SessionManager::with_timeout_and_warning(
            Duration::from_millis(100),
            Duration::ZERO,
        ));
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;

        let operation_guard = manager.begin_operation();
        sleep(Duration::from_millis(130)).await;

        let manager_for_lock = Arc::clone(&manager);
        let lock_task = tokio::spawn(async move {
            manager_for_lock.lock().await;
        });
        sleep(Duration::from_millis(50)).await;
        assert_eq!(manager.state().await, LifecycleState::Active);

        drop(operation_guard);
        timeout(Duration::from_millis(400), lock_task)
            .await
            .expect("manual lock task should complete")
            .expect("manual lock task must not panic");
        assert_eq!(manager.state().await, LifecycleState::Expired);

        let mut locked_events = 0usize;
        while let Ok(Ok(event)) = timeout(Duration::from_millis(120), receiver.recv()).await {
            if matches!(event, SessionEvent::Locked) {
                locked_events += 1;
            }
        }
        assert_eq!(
            locked_events, 1,
            "manual lock and timeout race should emit a single Locked event"
        );
    }

    #[tokio::test]
    async fn test_timeout_shorter_than_pre_warning_does_not_emit_warning() {
        let manager = SessionManager::with_timeout(Duration::from_millis(120));
        let mut receiver = manager.subscribe();
        authenticate_tier1(&manager).await;

        let first_event = timeout(Duration::from_millis(400), receiver.recv())
            .await
            .expect("timeout lock event should be emitted")
            .expect("event channel should stay open");
        assert!(matches!(first_event, SessionEvent::Locked));
        assert!(
            timeout(Duration::from_millis(100), receiver.recv())
                .await
                .is_err(),
            "no warning event should be emitted when timeout is shorter than pre-warning window"
        );
    }
}

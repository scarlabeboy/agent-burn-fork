//! Identities attached to every observation.
//!
//! Environment, harness and model provider are separate concepts: none is
//! derived from another, and none is derived from paths or display names.
//! Account identity is not represented here at all (see
//! [`crate::observation::AccountScope`]).

/// Maximum byte length of an [`OpaqueId`].
pub const OPAQUE_ID_MAX_LEN: usize = 64;
/// Maximum byte length of a [`WslDistroName`].
pub const WSL_DISTRO_NAME_MAX_LEN: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    InvalidOpaqueId,
    InvalidDistroName,
}

/// Install-local opaque identifier supplied by the caller (the future
/// coordinator). The boundary never mints, derives or reuses these.
///
/// Accepted form: `[A-Za-z0-9_-]{1,64}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueId(String);

impl OpaqueId {
    pub fn new(value: &str) -> Result<Self, IdentityError> {
        let valid = !value.is_empty()
            && value.len() <= OPAQUE_ID_MAX_LEN
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(IdentityError::InvalidOpaqueId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnvironmentKind {
    WindowsNative,
    Wsl2,
}

/// Registered WSL distribution name, as configured by the caller.
///
/// Accepted form: `[A-Za-z0-9._-]{1,64}`. Not verified against the WSL
/// registry: the boundary never starts or queries WSL.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WslDistroName(String);

impl WslDistroName {
    pub fn new(value: &str) -> Result<Self, IdentityError> {
        let valid = !value.is_empty()
            && value.len() <= WSL_DISTRO_NAME_MAX_LEN
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(IdentityError::InvalidDistroName)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The concrete environment instance behind an [`EnvironmentIdentity`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnvironmentInstance {
    WindowsHost,
    WslDistro(WslDistroName),
}

/// One collection environment. "Computer" is intentionally not an
/// environment: it is a later union view, never a collection source.
///
/// Equality includes the kind and instance, so a Windows and a WSL
/// environment are distinct even if a caller reuses the same opaque id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvironmentIdentity {
    id: OpaqueId,
    instance: EnvironmentInstance,
}

impl EnvironmentIdentity {
    pub fn windows(id: OpaqueId) -> Self {
        Self {
            id,
            instance: EnvironmentInstance::WindowsHost,
        }
    }

    pub fn wsl(id: OpaqueId, distro: WslDistroName) -> Self {
        Self {
            id,
            instance: EnvironmentInstance::WslDistro(distro),
        }
    }

    pub fn id(&self) -> &OpaqueId {
        &self.id
    }

    pub fn kind(&self) -> EnvironmentKind {
        match self.instance {
            EnvironmentInstance::WindowsHost => EnvironmentKind::WindowsNative,
            EnvironmentInstance::WslDistro(_) => EnvironmentKind::Wsl2,
        }
    }

    pub fn instance(&self) -> &EnvironmentInstance {
        &self.instance
    }

    pub fn wsl_distro(&self) -> Option<&WslDistroName> {
        match &self.instance {
            EnvironmentInstance::WindowsHost => None,
            EnvironmentInstance::WslDistro(distro) => Some(distro),
        }
    }
}

/// Coding-agent harness that produced a source. This is not the model
/// provider: a harness may route to several providers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Harness {
    ClaudeCode,
    Codex,
    OpenCode,
}

impl Harness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
        }
    }
}

/// One harness source root inside one environment, identified by a
/// caller-supplied opaque id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceInstance {
    id: OpaqueId,
    harness: Harness,
}

impl SourceInstance {
    pub fn new(id: OpaqueId, harness: Harness) -> Self {
        Self { id, harness }
    }

    pub fn id(&self) -> &OpaqueId {
        &self.id
    }

    pub fn harness(&self) -> Harness {
        self.harness
    }
}

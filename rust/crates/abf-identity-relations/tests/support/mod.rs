use abf_observation::{
    DeclaredContext, EnvironmentKind, ObservationEnvelope, OpaqueRef, SourceRunStatus,
    from_opencode,
};
use abf_parser::{Limits, parse};

pub fn id(text: &str) -> OpaqueRef {
    OpaqueRef::new(text).unwrap()
}

pub fn envelope(
    frame: &[u8],
    kind: EnvironmentKind,
    status: SourceRunStatus,
) -> ObservationEnvelope {
    from_opencode(
        parse(frame, Some("fictional_parser_claim"), Limits::MAX),
        DeclaredContext::new(
            kind,
            id("fictional_env"),
            id("fictional_source"),
            id("fictional_run"),
            Some(id("fictional_context_claim")),
            Some(u64::MAX),
        ),
        status,
    )
    .unwrap()
}

pub fn ordinary() -> ObservationEnvelope {
    envelope(br#"{"id":"fictional_message","sessionID":"fictional_session","tokens":{"input":7,"total":3}}"#, EnvironmentKind::Windows, SourceRunStatus::Complete)
}

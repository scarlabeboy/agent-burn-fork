use crate::*;

/// Fixed claim facts for every delivery, including quarantined conflicting claims.
/// Record payloads and parser results are absent; records exist only in RetainedRecord.
/// Unbound source deliveries omit source identity and source payload facts entirely.
#[derive(Clone)]
pub struct DeliveryEvidence {
    frame_seq: u32,
    source: Option<SourceRef>,
    source_seq: Option<u32>,
    terminal: Option<TerminalKind>,
    enumeration: Option<(usize, bool, RawEnumeration)>,
    enumerated_at: Option<ReaderInstant>,
}
impl DeliveryEvidence {
    pub fn enumerated_at(&self) -> Option<ReaderInstant> {
        self.enumerated_at
    }
    pub fn frame_seq(&self) -> u32 {
        self.frame_seq
    }
    pub fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }
    pub fn source_seq(&self) -> Option<u32> {
        self.source_seq
    }
    pub fn terminal(&self) -> Option<TerminalKind> {
        self.terminal
    }
    pub fn enumeration(&self) -> Option<(usize, bool, RawEnumeration)> {
        self.enumeration
    }
    pub(crate) fn from_frame(f: &ReaderFrame, retain_source: bool) -> Self {
        let (source, source_seq, terminal, enumeration) = match &f.payload {
            Payload::Enumeration {
                member,
                second,
                outcome,
                ..
            } => (None, None, None, Some((*member, *second, *outcome))),
            Payload::Source { .. } if !retain_source => (None, None, None, None),
            Payload::Source {
                source,
                source_seq,
                event,
            } => {
                let terminal = match event {
                    SourceEvent::Skipped(r) => Some(TerminalKind::Skipped(*r)),
                    SourceEvent::OpenFailed(r) => Some(TerminalKind::OpenFailed(*r)),
                    SourceEvent::Vanished => Some(TerminalKind::Vanished),
                    SourceEvent::ReadError(r) => Some(TerminalKind::ReadError(*r)),
                    SourceEvent::End { records, change } => Some(TerminalKind::End {
                        records: *records as u32,
                        change: *change,
                    }),
                    _ => None,
                };
                (
                    Some(SourceRef::new(source).expect("validated source")),
                    Some(*source_seq as u32),
                    terminal,
                    None,
                )
            }
            Payload::RunEnd => (None, None, None, None),
        };
        Self {
            frame_seq: f.frame_seq as u32,
            source,
            source_seq,
            terminal,
            enumeration,
            enumerated_at: match &f.payload {
                Payload::Enumeration { observed_at, .. } => *observed_at,
                _ => None,
            },
        }
    }
}
// Private transport equality key. No parser result is cloned into it. Admitted
// record bytes are necessary to classify byte-exact late replays; never exported.
#[derive(PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // Control branch is bounded and never contains a record.
pub(crate) enum IdentityPayload {
    Record {
        source: String,
        source_seq: u64,
        bytes: Vec<u8>,
    },
    Control(Payload),
}
impl IdentityPayload {
    pub(crate) fn from_payload(p: Payload) -> Self {
        match p {
            Payload::Source {
                source,
                source_seq,
                event: SourceEvent::Record { bytes, .. },
            } => Self::Record {
                source,
                source_seq,
                bytes,
            },
            control => Self::Control(control),
        }
    }
    pub(crate) fn matches(&self, p: &Payload) -> bool {
        match (self, p) {
            (
                Self::Record {
                    source,
                    source_seq,
                    bytes,
                },
                Payload::Source {
                    source: s,
                    source_seq: q,
                    event: SourceEvent::Record { bytes: b, .. },
                },
            ) => source == s && source_seq == q && bytes == b,
            (Self::Control(a), b) => a == b,
            _ => false,
        }
    }
}
redacted!(DeliveryEvidence, IdentityPayload);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abf107n_n16_private_transport_key_debug_redacts_both_variants() {
        let record = IdentityPayload::Record {
            source: "f1-private_marker".into(),
            source_seq: 1,
            bytes: b"private_marker".to_vec(),
        };
        let control = IdentityPayload::Control(Payload::Enumeration {
            member: 0,
            second: false,
            outcome: RawEnumeration::Closed(1),
            sources: vec!["f1-private_marker".into()],
            observed_at: None,
        });
        assert_eq!(format!("{record:?}"), "IdentityPayload([redacted])");
        assert_eq!(format!("{control:?}"), "IdentityPayload([redacted])");
    }
}
